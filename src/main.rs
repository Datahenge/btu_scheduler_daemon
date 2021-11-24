#![allow(unused_imports)]
#![allow(dead_code)]

use std::{
    collections::{HashMap, VecDeque},  // Used as a queue of BTU Task Schedule identifiers.
    os::unix::net::{UnixStream, UnixListener},
    sync::{Arc, Mutex, Barrier},
    time::{Duration, Instant},
};

use std::io;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::thread;
use std::thread::JoinHandle;

// Crates.io
use chrono::prelude::*;
use mysql::*;
use mysql::prelude::*;
use once_cell::sync::Lazy;

// This Crate
use pyrq_scheduler::{config, task_scheduler};
use pyrq_scheduler::config::AppConfig;

// Brian's GitHub Issue about this:
// https://github.com/aeshirey/aeshirey.github.io/issues/5

fn handle_socket_client(stream: UnixStream, queue: &mut VecDeque<String>) {
    let stream = BufReader::new(stream);
    for line in stream.lines() {
        let task_schedule_id = line.unwrap();
        println!("Adding value {} to the internal queue.", task_schedule_id);
        queue.push_back(task_schedule_id);
    }
}

fn queue_full_refill(queue: &mut VecDeque<String>) ->  Result<u32> {
    /*
        Purpose: Query MySQL, and add every Task Schedule ID to the queue.
        See also: https://docs.rs/mysql/21.0.2/mysql/index.html
    */

    /*  The syntax gets a bit wild below.  So here is the gist:
        Goal: We want to read the GLOBAL_CONFIG struct, which contains MySQL connection configuration data.
            1. Get a lock for GLOBALC_CONFIG. This yields a LockResult type.
            2. Unwrap that LockResult, to reveal a MutexGuard.
            3. We deference that MutexGuard, which yields an owned AppConfig.
            4. But there's no need to "move" AppConfig into 'get_mysql_conn()'.  We just needs a reference.  So use prefix '&'
    */

    let mut rows_added: u32 = 0;
    let mut conn = config::get_mysql_conn(&*GLOBAL_CONFIG.lock().unwrap())?;
    conn.query_iter("SELECT `name` FROM `tabBTU Task Schedule` ORDER BY name")
    .unwrap()
    .for_each(|row| {
        let r: String = from_row(row.unwrap());  // each value of r is a 'name' from the SQL table.
        queue.push_back(r);
        rows_added += 1;
    });

    Ok(rows_added)
}

fn get_datetime_now_string() -> String {
    Local::now().format("%v %r").to_string()
}

/* --------
Global Configuration:
Purpose: Initialize the application's global configuration.
Dev Note:  We create a Lazy Static, using a custom struct 'AppConfig', which was populated from a TOML file.---------
   -------- */
static GLOBAL_CONFIG: Lazy<Mutex<AppConfig>> = Lazy::new(|| {
    match AppConfig::new_from_toml_file() {
        Ok(app_config) => {
            Mutex::new(app_config)
        }
        Err(error) => {
            println!("Error while creating AppConfig from TOML configuration file.\n{}", error);
            std::process::exit(1);
        }
    }
});

fn main() {
    // Variables for for main()
    let checkmark_emoji = '\u{2713}';

    let mut handles: Vec<JoinHandle<()>> = Vec::with_capacity(3);  // We need 3 additional threads, besides the main thread.
    let id_queue: VecDeque<String> = VecDeque::new();  // an internal 'queue' containing BTU Task Schedule identifiers
    let queue_counter = Arc::new(Mutex::new(id_queue));  // Dev Note: this line "moves" the 'id_queue' into the Arc+Mutex.

    // Interval at which 'Next Execution Times' are examined, which may trigger RQ inserts.
    // Recommend a value of no-more-than 60 seconds.  Otherwise you risk missing a Cron Datetime.
    let scheduler_polling_interval: u64 =  GLOBAL_CONFIG.lock().unwrap().scheduler_polling_interval;  

    // Determines how often the internal queue receives a "full-refresh" of BTU Task Schedules from the MySQL database.
    // Suggested value is 60 minutes (3600 seconds)
    let max_seconds_between_updates: u32 = GLOBAL_CONFIG.lock().unwrap().max_seconds_between_updates;  
    
    // Sanity check.  If we cannot make a connection to Redis RQ, don't even bother continuing.
    if task_scheduler::get_redis_connection(&GLOBAL_CONFIG.lock().unwrap()).is_none() {
        println!("Cannot initialize daemon without Redis RQ connection; closing now.");
        std::process::exit(1);    
    }

    // ----------------
    // Thread #1:  Read FIFO values from internal database, and send to Redis Queue Database.
    // ----------------
    let counter = Arc::clone(&queue_counter);
    let thread_handle_1 = thread::spawn(move || {
        loop {
            // Attempt to acquire a lock:
            if let Ok(mut unlocked_queue) = counter.lock() {
                // Lock acquired.
                if ! (*unlocked_queue).is_empty() {
                    // Pop the next value out of the queue (FIFO)
                    let next_task_schedule_id = (*unlocked_queue).pop_front().unwrap();
                    println!("{} : Popped value '{}' from queue.  Building CRON data and transmitting to Redis Queue Database.",
                              get_datetime_now_string(), next_task_schedule_id);

                    let sql_result =  task_scheduler::read_btu_task_schedule(&*GLOBAL_CONFIG.lock().unwrap(), &next_task_schedule_id);
                    // Match on the results:
                    if let Some(btu_task_schedule) = sql_result {
                        // We now have an owned struct BtuTaskSchedule.
                        let _foo = task_scheduler::add_task_schedule_to_rq(&*GLOBAL_CONFIG.lock().unwrap(), &btu_task_schedule);
                    } else {
                        // Destructure failed. Change to the failure case.
                        println!("Error: Was unable to read the SQL database and find a record for BTU Task Schedule = '{}'", next_task_schedule_id);
                    }                              
                    // println!("{} values remain in internal queue.", (*unlocked_queue).len());
                }
            }
            thread::sleep(Duration::from_millis(1250));  // Yield control to another thread.
        }
    });
    handles.push(thread_handle_1);

    // ----------------
    // Thread #2:  Repopulate the queue with values every N seconds.
    // ----------------

    let counter2 = Arc::clone(&queue_counter);
    let thread_handle_2 = thread::spawn(move || {

        let mut stopwatch: Instant = Instant::now();  // use to keep track of time elapsed.
        loop {
            let elapsed_seconds = stopwatch.elapsed().as_secs();  // calculate elapsed seconds since last Queue Repopulate

            // Check if enough time has passed.
            if elapsed_seconds > max_seconds_between_updates.into() {  // Dev Note: The 'into()' handles conversion to u64
                if let Ok(mut unlocked_queue) = counter2.lock() {
                    // Achieved a lock.
                    println!("{} seconds have elapsed.  Time to fill up the queue!", elapsed_seconds);                    
                    println!("Before refill, the queue contains {} values.", (*unlocked_queue).len());
                    match queue_full_refill(&mut *unlocked_queue) {
                        Ok(rows_added) => {
                            println!("Added {} values to the queue.", rows_added);
                            println!("After Repopulation: Queue has {} values.", (*unlocked_queue).len());
                            stopwatch = Instant::now()  // reset the stopwatch, and begin new countdown.
                        },
                        Err(e) => println!("Error while repopulating the queue! {:?}", e)
                    }                       
                }
            }
            thread::sleep(Duration::from_millis(750));  // Yield control to another thread.
        } // end of loop
    });
    handles.push(thread_handle_2);


    // ----------------
    // Thread #3:  Enqueue Tasks into RQ
    //
    // This is the fundamental thread that reviews the "Next Execution Time" for every BTU Task Schedule.
    // And if that datetime has past?  It converts the BTU Task into an RQ Job.  And places in RQ's "Execute This Immediately" queue.
    // ----------------

    // let counter3 = Arc::clone(&queue_counter);
    let thread_handle_3 = thread::spawn(move || {  // need this 'move' to own variable 'scheduler_polling_interval'

        loop {
            let stopwatch: Instant = Instant::now();
            task_scheduler::promote_jobs_to_rq_if_ready(&GLOBAL_CONFIG.lock().unwrap());
            let elapsed_seconds = stopwatch.elapsed().as_secs();  // time just spent working on RQ database.
            // Subtract the Time Elapsed from the desired Wait Time.  This is the remaining time this thread should sleep.
            thread::sleep(Duration::from_secs(scheduler_polling_interval - elapsed_seconds)); // wait N seconds before trying again.
        }
    });
    handles.push(thread_handle_3);

    // ----------------
    // Main Thread:  a Unix Domain Socket listener.
    // ----------------

    println!("-------------------------------------");
    println!("Rusty BTU Scheduler: by Datahenge LLC");
    println!("-------------------------------------");

    println!("\nThis daemon performs the following functions:");
    println!("1. Listens on Unix Domain Socket for traffic from Frappe BTU web application.");
    println!("2. Updates the Redis Queue database with latest BTU Task Schedule data.");
    println!("3. Performs a full-refresh of BTU Task Schedules every {} seconds.\n", max_seconds_between_updates);

    println!("{} Application started at {}", checkmark_emoji, get_datetime_now_string());

    // 1. Populate the internal queue with values on startup.
    let counter = Arc::clone(&queue_counter);
    {
        let mut unlocked_queue = counter.lock().unwrap();
        let rows_added = queue_full_refill(&mut unlocked_queue).unwrap();
        println!("{} Initialized internal queue with {} values.", checkmark_emoji, rows_added);
    } // scope ensures that lock is dropped immediately.

    // 2. Establish path to Unix Domain Socket file; delete if file exists from previous executions.
    let socket_path = Path::new("/tmp/pyrq_scheduler.sock");
    if socket_path.exists() {
        // delete the socket file, if it exists:
        std::fs::remove_file(socket_path)
            .expect(&format!("ERROR: Could not remove file '{}'", socket_path.to_string_lossy()));
    }

    // 3. Listen for incoming client traffic on Unix Domain Socket
    let listener = UnixListener::bind("/tmp/pyrq_scheduler.sock").unwrap();
    println!("{} Listening for inbound traffic on 'pyrq_scheduler.sock'", checkmark_emoji);
    for stream in listener.incoming() {
        let counter3 = Arc::clone(&queue_counter);
        match stream {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Ok(mut unlocked_queue) = counter3.lock() {
                        handle_socket_client(stream, &mut *unlocked_queue); // pushes Socket Client's data into internal queue.
                    }  // end of locked section.
                    // thread::sleep(Duration::from_millis(1250));  // Yield control to another thread.
                });
            }
            Err(err) => {
                println!("Error: {}", err);
                break;
            }
        }
    };

    // Join all the handles together...
    for handle in handles {
        let _ = handle.join();
    }

    println!("Warning: This line should never print.");
}
