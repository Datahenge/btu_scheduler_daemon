#![forbid(unsafe_code)]

use std::{
    collections::{VecDeque},  // VecDeque used as an internal queue, holding BTU Task Schedule identifiers.  
    os::unix::net::{UnixListener},
    sync::{Arc, Mutex},  // Barrier
    thread,
    time::{Duration, Instant},
    env
};

// Crates.io
use chrono::prelude::*;
use mysql::Result as mysqlResult;
use mysql::prelude::Queryable;
use once_cell::sync::Lazy;

// This Crate
use btu_scheduler::{config, rq, scheduler, task_schedule};
use btu_scheduler::config::AppConfig;
pub mod ipc_stream;
pub mod common;


// GitHub Issue where Brian and Adam discuss Rust thread locking:
// https://github.com/aeshirey/aeshirey.github.io/issues/5

/**
 Queries the Frappe database, adding every Task Schedule ID to the Scheduler's internal queue.\
 This effectively performs a "full refresh" in Python RQ.
*/
fn queue_full_refill(queue: &mut VecDeque<String>) ->  mysqlResult<u32> {
    // For more information on the Rust mysql crate: https://docs.rs/mysql/latest/mysql/index.html

    let mut rows_added: u32 = 0;
    
    /*  The next line below is a bit wild.  Here is the concept:

        Goal: Read the APP_CONFIG struct, to obtain information about how to connect to MySQL/MariaDB.

        1. First, get a lock for APP_CONFIG. This yields a LockResult type.
        2. Unwrap that LockResult, to reveal a MutexGuard.
        3. By deferencing the MutexGuard using *, we yield an -owned- AppConfig.
        4. However...there's no need to -move- AppConfig into 'get_mysql_conn()'.  We just need a reference.  So prefix with '&'
    */

    let mut conn = config::get_mysql_conn(&*APP_CONFIG.lock().unwrap())?;

    conn.query_iter("SELECT `name` FROM `tabBTU Task Schedule` WHERE enabled = 1 ORDER BY name;")
    .unwrap()
    .for_each(|row_result| {
        match row_result {
            Ok(row) => {
                let r: String = mysql::from_row(row);  // each value of r is a 'name' from the SQL table.  The primary key of BTU Task Schedule .
                queue.push_back(r);
                rows_added += 1;
            },
            Err(error) => {
                println!("Error with SQL row result: {:?}", error);
            }
        }
    });
    Ok(rows_added)
}

/**
 Return the current, local time as String. 
*/
fn get_datetime_now_string() -> String {
    Local::now().format("%v %r").to_string()
}

/**
 The global configuration for this application.\
 Developer Note:  We need to create a Lazy Static, using a custom struct 'AppConfig', populated from a TOML file.\
 Why a Lazy Static?  So we can pass this configuration struct between threads!
*/
static APP_CONFIG: Lazy<Mutex<AppConfig>> = Lazy::new(|| {
    match AppConfig::new_from_toml_file() {
        Ok(app_config) => {
            if app_config.tz().is_err() {
                println!("Cannot parse time zone string in TOML configuration file: '{}'", app_config.time_zone_string);
                println!("See this article for a list of valid names:\n{}", "https://en.wikipedia.org/wiki/List_of_tz_database_time_zones");
                std::process::exit(1);
            }
            Mutex::new(app_config)
        }
        Err(error) => {
            println!("Error while creating AppConfig from TOML configuration file.\n{}", error);
            std::process::exit(1);
        }
    }
});

fn main() {

    // Check if called with --version.  If so, display some information, then exit.
    // rustc --check-cfg 'values(foo, "red", "green")' --check-cfg 'values(bar, "up", "down")'};
    let args: Vec<String> = env::args().collect();
    if args.len() == 2 && &args[1] == "--version" {
        println!("Version: {}", btu_scheduler::get_package_version());
        println!("Linux Distribution: {}", common::target_linux_distro());
        std::process::exit(0);  // exit cleanly
    }

    let checkmark_emoji = '\u{2713}';
    let mut handles: Vec<thread::JoinHandle<()>> = Vec::with_capacity(3);  // We need 3 additional threads, besides the main thread.
    // Create a new VecDeque, and -move- into an ArcMutex.  This allows the queue to be passed between threads.
    let internal_queue = Arc::new(Mutex::new(VecDeque::<String>::new()));  // using a 'turbofish' to specify the type of the VecDeque (we want Strings)
    
    // Lock the APP_CONFIG for just a moment, to populate some immutable variables.
    let temp_app_config = APP_CONFIG.lock().unwrap();

    /*
      The interval at which 'Next Execution Times' are examined, to potentially trigger RQ inserts.\
      I recommend a value of no-more-than 60 seconds.  Otherwise you risk missing a Cron Datetime.
    */ 
    let scheduler_polling_interval: u64 =  temp_app_config.scheduler_polling_interval;  

    /* Below is the interval for performing a "full-refresh" of BTU Task Schedules from the MySQL database.
       A good value might be 3600 seconds (60 minutes)
    */
    let full_refresh_internal_secs: u32 = temp_app_config.full_refresh_internal_secs;  
    
    /* The statement below is basically a sanity check.  If we cannot successfully connnect to Redis RQ on startup?
       The daemon cannot do anything, and should terminate.  This can be tempered with a Restart clause in Systemd Unit Files,
       to handle race conditions on server boot.
    */
    if rq::get_redis_connection(&temp_app_config).is_none() {
        println!("Cannot initialize daemon without Redis RQ connection; closing now.");
        std::process::exit(1);    
    }

    // Another sanity check; try to connect to SQL before going any further.
    match btu_scheduler::validate_sql_credentials(&temp_app_config) {
        Ok(_) => {
        },
        Err(error) => {
            println!("{}", error);
            println!("Cannot initialize daemon without connection to MySQL database; closing now.");
            std::process::exit(1);    
        }
    }

    // Finished reading APP_CONFIG, so we should release the lock.
    drop(temp_app_config);

    /*
      ----------------
       Thread #1:  This thread reads the Internal Queue in a FIFO manner.
                   For each Task Schedule ID found:
                   1.  Write the "Next Execution Times" to the Python RQ (Redis Queue) database.
                   2.  Nothing else; do not attempt to build an RQ Job at this time.  This is a deliberate design decision by the author.
      ----------------
    */
    let queue_counter_1 = Arc::clone(&internal_queue);
    let thread_handle_1 = thread::Builder::new().name("1_Internal_Queue".to_string()).spawn(move || {
        loop {
            // Attempt to acquire a lock
            if let Ok(mut unlocked_queue) = queue_counter_1.lock() {
                // Lock acquired.
                if ! (*unlocked_queue).is_empty() {

                    let next_task_schedule_id: String;
                    match (*unlocked_queue).pop_front() {  // Pop the next value out of the queue (FIFO)
                        Some(value) => {
                            next_task_schedule_id = value;
                            // dbg!("{} : Popped value '{}' from internal queue.  Calculating next execution times, and writing them in RQ Database.",
                            // get_datetime_now_string(), next_task_schedule_id);

                            if let Ok(unlocked_app_config) = APP_CONFIG.lock() {
                                let sql_result =  task_schedule::read_btu_task_schedule(&*unlocked_app_config, &next_task_schedule_id);
                                if let Some(btu_task_schedule) = sql_result {
                                    // We now have an owned struct BtuTaskSchedule.
                                    let _foo = scheduler::add_task_schedule_to_rq(&*unlocked_app_config, &btu_task_schedule);
                                } else {
                                    println!("Error: Unable to find SQL record for BTU Task Schedule = '{}'\n(verify BTU Configuration has a Time Zone)", next_task_schedule_id);
                                }                              
                            }
                            println!("{} values remain in internal queue.", (*unlocked_queue).len());
                        },
                        None => {
                        }
                    }

                }
            }
            thread::sleep(Duration::from_millis(1250));  // Yield control to another thread.
        }
    });
    if thread_handle_1.is_err() {
        println!("Cannot spawn new thread '1_Internal_Queue'.  Error information below.  Ending program.");
        println!("{:?}", thread_handle_1.err());
        std::process::exit(1);    
    }
    handles.push(thread_handle_1.unwrap());

    /*
      ----------------
       Thread #2:  Every N seconds, refill the Internal Queue with -all- Task Schedule IDs.
                   Once finished, thread #1 will begin processing them one at a time.

                   This is a type of "safety net" for the BTU system.  By performing a "full refresh" of RQ,
                   we can be confident that Tasks are always running.  Even if the RQ database is flushed or emptied,
                   it will be refilled automatically after a while!
      ----------------
    */
    let queue_counter_2 = Arc::clone(&internal_queue);
    let thread_handle_2 = thread::Builder::new().name("2_Auto_Refill".to_string()).spawn(move || {

        let mut stopwatch: Instant = Instant::now();  // used to keep track of time elapsed.
        loop {
            let elapsed_seconds = stopwatch.elapsed().as_secs();  // calculate elapsed seconds since last Queue Repopulate
            // Check if enough time has passed...
            if elapsed_seconds > full_refresh_internal_secs.into() {  // Dev Note: The 'into()' handles conversion to u64
                // println!("Thread 2: Attempting to acquire a lock on the internal queue...");
                if let Ok(mut unlocked_queue) = queue_counter_2.lock() {
                    // println!("Thread 2 unlocked.");
                    // Achieved a lock.
                    println!("--------\n{} seconds have elapsed.  It's time for a full-refresh of the Task Schedules in Redis!", elapsed_seconds);                    
                    println!("  * Before refill, the queue contains {} values.", (*unlocked_queue).len());
                    match queue_full_refill(&mut *unlocked_queue) {
                        Ok(rows_added) => {
                            println!("  * Added {} values to the internal FIFO queue.", rows_added);
                            println!("  * Internal queue contains a total of {} values.", (*unlocked_queue).len());
                            stopwatch = Instant::now()  // reset the stopwatch, and begin new countdown.
                        },
                        Err(e) => println!("Error while repopulating the internal queue! {:?}", e)
                    }                       
                    println!("--------")
                }
            }
            thread::sleep(Duration::from_millis(750));  // Yield control to another thread for a while.
        } // end of loop
    });
    if thread_handle_2.is_err() {
        println!("Cannot spawn new thread '2_Auto_Refill'.  Error information below.  Ending program.");
        println!("{:?}", thread_handle_2.err());
        std::process::exit(1);    
    }
    handles.push(thread_handle_2.unwrap());

    /*
      ----------------
      Thread #3:  Enqueue Tasks into RQ
      
       Every N seconds, examine the Next Execution Time for all scheduled RQ Jobs (this information is stored in RQ as a Unix timestamps)
       If the Next Execution Time is in the past?  Then place the RQ Job into the appropriate queue.  RQ and Workers take over from there.
      ----------------
    */
    
    let queue_counter_3 = Arc::clone(&internal_queue);
    let thread_handle_3 = thread::Builder::new().name("3_Scheduler".to_string()).spawn(move || {  // this 'move' is required to own variable 'scheduler_polling_interval'
        thread::sleep(Duration::from_secs(10)); // One-time delay of execution: this gives the other Threads a chance to initialize.
        println!("--> Thread '3_Scheduler' launched.  Eligible RQ Jobs will be placed into RQ Queues at the appropriate time.");
        loop {
            // This thread requires a lock on the Internal Queue, so that after a Task runs, it can be rescheduled.
            let stopwatch: Instant = Instant::now();
            if let Ok(mut unlocked_queue) = queue_counter_3.lock() {
                // Successfully achieved a lock on the queue.
                if let Ok(app_config) = &APP_CONFIG.lock() {
                    // Successfully achieved a lock on the Application Configuration.
                    scheduler::check_and_run_eligible_task_schedules(app_config, &mut *unlocked_queue);
                }
            }
            let elapsed_seconds = stopwatch.elapsed().as_secs();  // time just spent working on RQ database.
            // I want this thread to execute at roughly the same interval.
            // Bu subtracting the Time Elapsed above, from the desired Wait Time, we know how much longer the thread should sleep.
            thread::sleep(Duration::from_secs(scheduler_polling_interval - elapsed_seconds)); // wait N seconds before trying again.
        }
    });
    if thread_handle_3.is_err() {
        println!("Cannot spawn new thread '3_Scheduler'.  Error information below.  Ending program.");
        println!("{:?}", thread_handle_3.err());
        std::process::exit(1);    
    }
    handles.push(thread_handle_3.unwrap());

    // ----------------
    // Main Thread:  a Unix Domain Socket listener.
    // ----------------

    println!("-------------------------------------");
    println!("BTU Scheduler: by Datahenge LLC");
    println!("-------------------------------------");

    println!("\nThis daemon performs the following functions:\n");
    println!("1. Performs the role of a Scheduler, enqueuing BTU Task Schedules in Python RQ whenever it's time to run them.");
    println!("2. Performs a full-refresh of BTU Task Schedules every {} seconds.", full_refresh_internal_secs);    
    println!("3. Listens on Unix Domain Socket for requests from the Frappe BTU web application.\n");

    println!("{} Main Thread started at {}", checkmark_emoji, get_datetime_now_string());

    // Immediately on startup, Scheduler daemon should populate its internal queue with all BTU Task Schedule identifiers.
    let queue_counter_temp = Arc::clone(&internal_queue);
    {
        // Note: using an explicit scope here, to ensure the lock is dropped immediately afterwards, so new threads can take it.
        let mut unlocked_queue = queue_counter_temp.lock().unwrap();
        let rows_added = queue_full_refill(&mut unlocked_queue).unwrap();
        println!("{} Filled internal queue with {} Task Schedule identifiers.", checkmark_emoji, rows_added);
        drop(unlocked_queue);
    }

    // The purpose of the main() thread = Unix Domain Socket server!
    let listener: UnixListener = ipc_stream::create_socket_listener(&APP_CONFIG.lock().unwrap().socket_path);
    {
        // After creating the UDS file, Linux requires we change the file permissions:
        // NOTE: Wrapping in a smaller namespace, so APP_CONFIG is automatically unlocked.
        let unlocked_app_config: &AppConfig = &APP_CONFIG.lock().unwrap();
        match ipc_stream::update_socket_file_permissions(&unlocked_app_config.socket_path, &unlocked_app_config.socket_file_group_owner) {
            Ok(_) => {
                println!("{} Successfully updated Unix Domain Socket file's permissions.", checkmark_emoji);
            },
            Err(error) => {
                println!("\nERROR: Failed to modify Unix Domain Socket file's permissions:\n    {}", error);
                println!("Frappe Web App would be unable to send commands to the BTU Scheduler.\nEnding daemon now.");
                std::process::exit(1);
            }
        }
    }

    for stream in listener.incoming() {
        let queue_counter_main = Arc::clone(&internal_queue);
        match stream {
            Ok(unwrapped_stream) => {
                let handler_result = thread::Builder::new().name("Unix_Socket_Handler".to_string()).spawn(move || {
                    // Call a function to handle whatever request is being made by a remote Client.
                    let request_result = ipc_stream::handle_client_request(unwrapped_stream, 
                                                                           queue_counter_main,
                                                                           &APP_CONFIG.lock().unwrap());
                    if let Err(error_message) = request_result {
                        println!("Error while handling Unix client stream: {}", error_message);
                    }
                    thread::sleep(Duration::from_millis(1250));  // Yield control to another thread.
                });
                if handler_result.is_err() {
                    println!("Error in thread 'Unix_Socket_Handler': {:?}", handler_result.err());
                }
            }
            Err(err) => {
                println!("Error while attempting to unwrap UnixListener stream: {}.  Will keep listening for more traffic.", err);
            }
        }
    };
}
