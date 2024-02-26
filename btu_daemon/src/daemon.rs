#![forbid(unsafe_code)]
#![allow(unused_imports)]

use std::{collections::VecDeque,
          env,
          fmt::Debug,
          os::unix::net::UnixListener,
          sync::{Arc, Mutex ,MutexGuard},
          thread,
          time::{Duration, Instant}};

// Crates.io
use chrono::prelude::*;
use mysql::Result as mysqlResult;
use mysql::prelude::Queryable;
use once_cell::sync::Lazy;

// Tracing modules
use tracing::{trace, debug, info, warn, error, span, Level};
use tracing::dispatcher::Dispatch;
use tracing_subscriber::{FmtSubscriber, Registry, filter, prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt};

// This Crate
pub mod common;
pub mod ipc_stream;
pub mod logging;
use btu_scheduler::{config, rq, scheduler, task_schedule};
use btu_scheduler::config::AppConfig;
use logging::CustomLayer;

// GitHub Issue where Brian and Adam discuss Rust thread locking: https://github.com/aeshirey/aeshirey.github.io/issues/5

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
                error!("Error with SQL row result: {:?}", error);
            }
        }
    });
    Ok(rows_added)
}

/**
 The global configuration for this application.\
 Developer Note:  We need to create a Lazy Static, using a custom struct 'AppConfig', populated from a TOML file.\
 Why a Lazy Static?  So we can pass this configuration struct between threads!
*/
static APP_CONFIG: Lazy<Mutex<AppConfig>> = Lazy::new(|| {
    // TODO: Need to parse arguments to Daemon for path to configuration file.
    match AppConfig::new_from_toml_file(None) {
        Ok(app_config) => {
            if app_config.tz().is_err() {
                error!("Cannot parse time zone string in TOML configuration file: '{}' 
                See this article for a list of valid names: https://en.wikipedia.org/wiki/List_of_tz_database_time_zones", app_config.time_zone_string);
                std::process::exit(1);
            }
            Mutex::new(app_config)
        }
        Err(error) => {
            error!("Error while creating AppConfig from TOML configuration file. {}", error);
            std::process::exit(1);
        }
    }
});


fn test_configuration_file() {
      /*
        Challenge: We need to load the TOML configuration from disk.
        * There could be errors (missing keys)
        * The errors are captured by tracing.
        * The TOML configuration specifies what tracing Levels are filtered.

        Catch 22 : We cannot validate the contents without tracing, which cannot be initialized without the contents.

        Solution:
        1. Inside a scope, initialize Tracing in INFO mode.
        2. Read the TOML configuration file.
        3. If there are errors, they will be output.  And the program will close (error code 1)
        4. Otherwise, exit the scope.  And reload it for real.

        TODO: Would be great to do this in 1 single pass, but I haven't learned if/how that's possible.
    */
    
    /*
        Subscribers do nothing, unless they are the default.  There are 2 ways of doing this:
        1.  Globally via `set_global_default`
        2.  Within a scope, using `with_default`

        The dispatcher is the component of the tracing system which is responsible for forwarding trace data
        from the instrumentation points that generate it to the Subscriber that collects it.
    */
    use tracing::dispatcher::Dispatch;
    let my_subscriber = Registry::default().with(CustomLayer);
    let my_dispatch = Dispatch::new(my_subscriber);
    tracing::dispatcher::with_default(&my_dispatch, || {

        // NOTE: I previous had '_', but the compiler actually wants a named variable, as of February 25th 2024.
        let _foo = APP_CONFIG.lock().unwrap();  // Lock APP_CONFIG for a moment, to populate some immutable variables.

    });
}


fn main() {

    // when the daemon is called with argument '--version', display some information, then exit.
    let args: Vec<String> = env::args().collect();
    if (args.len() == 2) && (&args[1] == "--version") {
        println!("Version: {}", btu_scheduler::get_package_version());
        println!("Linux Distribution: {}", common::target_linux_distro());
        std::process::exit(0);  // exit with success code
    }

    test_configuration_file();  // ensure the TOML configuration file meets the struct's requirements.
    let temp_app_config: MutexGuard<AppConfig> =  APP_CONFIG.lock().unwrap();  // lock the configuration for a while during initialization.

    // Initialize tracing globally.  For the remainder of the program, avoid using the println! macro.
    tracing_subscriber::registry()
        .with(CustomLayer)
        .with(temp_app_config.tracing_level.get_level())
        .init();

    let mut handles: Vec<thread::JoinHandle<()>> = Vec::with_capacity(3);  // Daemon requires 3 additional thread handles, besides the main thread.
    /*  Create a new VecDeque, and -move- into an ArcMutex.  This enables the Internal Queue to be passed between threads.
    */
    let internal_queue = Arc::new(Mutex::new(VecDeque::<String>::new()));  // using a 'turbofish' to specify the type of the VecDeque (String in this case)

    /*
      The interval at which 'Next Execution Times' are examined, to potentially trigger RQ inserts.\
      I recommend a value of no-more-than 60 seconds.  Otherwise you risk missing a Cron Datetime.
    */ 
    let scheduler_polling_interval: u64 =  temp_app_config.scheduler_polling_interval;  

    /* The interval for performing a "full-refresh" of BTU Task Schedules from the MySQL database.
       A good value might be 3600 seconds (60 minutes)
    */
    let full_refresh_internal_secs: u32 = temp_app_config.full_refresh_internal_secs;  
    
    /* The statement below is basically a sanity check.  If we cannot successfully connnect to Redis RQ on startup?
       The daemon cannot do anything, and should terminate.  This can be tempered with a Restart clause in Systemd Unit Files,
       to handle race conditions on server boot.

       February 25th 2024 - Allow the app to startup without failing on these conditions.
    */
    if rq::get_redis_connection(&temp_app_config, false).is_none() {
        if temp_app_config.startup_without_database_connections {
            warn!("Application is configured to startup without establishing a connection to Redis.");
        } else {
            error!("Cannot initialize daemon without an active Redis RQ connection; closing now.");
            std::process::exit(1);
        }
    }

    // Another sanity check; try to connect to SQL before going any further.
    match btu_scheduler::validate_sql_credentials(&temp_app_config) {
        Ok(_) => {
        },
        Err(error) => {
            error!("{}", error);
            error!("Unable to establish a connection Frappe MySQL database.");
            if ! temp_app_config.startup_without_database_connections {
                std::process::exit(1);
            }
        }
    }

    // Finished reading APP_CONFIG, so we should release the lock.
    drop(temp_app_config);

    /*
      ----------------
       Thread #1:  This thread reads the Internal Queue in a FIFO manner.
                   For each Task Schedule ID found:
                   1.  Write the "Next Execution Times" to the Python RQ (Redis Queue) database using zadd.
                   2.  Nothing else.
                   3.  Do NOT attempt to construct an RQ Job in-advance.  (deliberate design decision by the author)
      ----------------
    */
    let queue_counter_1 = Arc::clone(&internal_queue);
    let thread_handle_1 = thread::Builder::new().name("1_Internal_Queue".to_string()).spawn(move || {
        loop {
            debug!("Thread 1: Reading from Internal Queue...");
            // Attempt to acquire a lock...
            if let Ok(mut unlocked_queue) = queue_counter_1.lock() {
                // ...lock acquired.
                if ! (*unlocked_queue).is_empty() {

                    match (*unlocked_queue).pop_front() {  // Pop the next value out of the queue (FIFO)
                        Some(value) => {
                            let next_task_schedule_id: String = value;  // BTU Task Schedule 'name'
                            if let Ok(unlocked_app_config) = APP_CONFIG.lock() {
                                let sql_result =  task_schedule::read_btu_task_schedule(&*unlocked_app_config, &next_task_schedule_id);
                                if let Some(btu_task_schedule) = sql_result {
                                    // We now have an owned struct BtuTaskSchedule.
                                    let _foo = scheduler::add_task_schedule_to_rq(&*unlocked_app_config, &btu_task_schedule);
                                } else {
                                    error!("Error: Unable to find SQL record for BTU Task Schedule = '{}'\n(verify BTU Configuration has a Time Zone)", next_task_schedule_id);
                                }                              
                            }
                            trace!("{} values remain in internal queue.", (*unlocked_queue).len());
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
        error!("Cannot spawn new thread '1_Internal_Queue'.  Error information below.  Ending program.");
        error!("{:?}", thread_handle_1.err());
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
            debug!("Thread 2: Attempting to Auto-Refill the Internal Queue...");
            let elapsed_seconds = stopwatch.elapsed().as_secs();  // calculate elapsed seconds since last Queue Repopulate
            // Check if enough time has passed...
            if elapsed_seconds > full_refresh_internal_secs.into() {  // Dev Note: The 'into()' handles conversion to u64
                // trace!("Thread 2: Attempting to acquire a lock on the internal queue...");
                if let Ok(mut unlocked_queue) = queue_counter_2.lock() {
                    // trace!("Thread 2 unlocked.");
                    // Achieved a lock.
                    info!("{} seconds have elapsed.  It's time for a full-refresh of the Task Schedules in Redis!", elapsed_seconds);                    
                    debug!("  * Before refill, the queue contains {} values.", (*unlocked_queue).len());
                    match queue_full_refill(&mut *unlocked_queue) {
                        Ok(rows_added) => {
                            debug!("  * Added {} values to the internal FIFO queue.", rows_added);
                            debug!("  * Internal queue contains a total of {} values.", (*unlocked_queue).len());
                            stopwatch = Instant::now();  // reset the stopwatch, and begin new countdown.

                            // Log the Task Schedule:
                            if let Ok(unlocked_app_config) = APP_CONFIG.lock() {
                                crate::scheduler::rq_print_scheduled_tasks(&unlocked_app_config, false);      
                            }
                        },
                        Err(e) => error!("Error while repopulating the internal queue! {:?}", e)
                    }                       
                }
            }
            thread::sleep(Duration::from_millis(750));  // Yield control to another thread for a while.
        } // end of loop
    });
    if thread_handle_2.is_err() {
        error!("Cannot spawn new thread '2_Auto_Refill'.  Error information below.  Ending program. {:?}", thread_handle_2.err());
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
        info!("--> Thread '3_Scheduler' has launched.  Eligible RQ Jobs will be placed into RQ Queues at the appropriate time.");
        loop {
            debug!("Thread 3: Attempting to add new Jobs to RQ...");
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
        error!("Cannot spawn new thread '3_Scheduler'.  Error information below.  Ending program. {:?}", thread_handle_3.err());
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

    info!("Main Thread started");

    // TODO: Would be lovely if the main thread knew about the child threads status?
    // https://stackoverflow.com/questions/35883390/how-to-check-if-a-thread-has-finished-in-rust

    // Immediately on startup, Scheduler daemon should populate its internal queue with all BTU Task Schedule identifiers.
    let queue_counter_temp = Arc::clone(&internal_queue);
    {
        // Note: using an explicit scope here, to ensure the lock is dropped immediately afterwards, so new threads can take it.
        let mut unlocked_queue = queue_counter_temp.lock().unwrap();

        match queue_full_refill(&mut unlocked_queue) {
            Ok(rows_added) => {
                info!("Filled internal queue with {} Task Schedule identifiers.", rows_added);                
            },
            Err(error) => {
                warn!("{}", error);
                warn!("Unable to establish a connection Frappe MySQL database.");
                // std::process::exit(1);    
            }
        }
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
                trace!("Successfully updated Unix Domain Socket file's permissions.");
            },
            Err(error) => {
                error!("\nERROR: Failed to modify Unix Domain Socket file's permissions:\n    {}", error);
                error!("Frappe Web App would be unable to send commands to the BTU Scheduler.\nEnding daemon now.");
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
                        error!("Error while handling Unix client stream: {}", error_message);
                    }
                    thread::sleep(Duration::from_millis(1250));  // Yield control to another thread.
                });
                if handler_result.is_err() {
                    error!("Error in thread 'Unix_Socket_Handler': {:?}", handler_result.err());
                }
            }
            Err(err) => {
                error!("Error while attempting to unwrap UnixListener stream: {}.  Will keep listening for more traffic.", err);
            }
        }
    };
}


// let checkmark_emoji = '\u{2713}';