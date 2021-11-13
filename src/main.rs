

#![allow(unused_imports)]
#![allow(dead_code)]

use std::collections::VecDeque;  // Used as a queue of BTU Task Schedule identifiers.
use std::io;
use std::io::{BufRead, BufReader};
use std::os::unix::net::{UnixStream,UnixListener};
use std::path::Path;
use std::thread;
use std::time::Duration;

use chrono::prelude::*;
use mysql::*;
use mysql::prelude::*;

fn handle_client(stream: UnixStream) {
    let stream = BufReader::new(stream);
    for line in stream.lines() {
        println!("From Unix Domain Socket stream: {}", line.unwrap());
    }
}

fn queue_full_refill(queue: &mut VecDeque<String>) ->  Result<()> {
    /*
        Query MySQL, and add every Task Schedule ID to the queue.
     */

    // https://docs.rs/mysql/21.0.2/mysql/index.html
    // For now, just push some Strings, until we figure out the MySQL portion.
    queue.push_back("foo".to_owned());
    queue.push_back("bar".to_owned());
    queue.push_back("baz".to_owned());

    let url = "mysql://root:echo3@sevenFTP@localhost:3313/v13testdb";
    let opts = Opts::from_url(url)?;
    let pool = Pool::new(opts)?;
    
    let mut conn = pool.get_conn()?;

    conn.query_iter("SELECT `name` FROM `tabBTU Task Schedule`")
    .unwrap()
    .for_each(|row| {
        let r: String = from_row(row.unwrap());
        println!("SQL Row = {}", r);
    });

    Ok(())
}


fn main() {
    // Globals:

    // https://doc.rust-lang.org/std/collections/struct.VecDeque.html
    let mut handles = vec![];

    let mut id_queue: VecDeque<String> = VecDeque::new();
    
    // let mut seconds_since_full_update: Option<u64> = None;
    
    let socket_path = Path::new("/tmp/pyrq_scheduler.sock");

    // Delete the socket file, if it exists:
    if socket_path.exists() {
        std::fs::remove_file(socket_path).expect("ERROR: Could not remove 'pyrq_scheduler.sock'");    
    }
    
    // let listener = UnixListener::bind("/tmp/pyrq_scheduler.sock").unwrap();

    // Initialize the queue:
    match queue_full_refill(&mut id_queue) {
        Ok(v) => println!("Queue was refilled okay: {:?}", v),
        Err(e) => println!("Error while filling queue! {:?}", e),
    }

    println!("Size of Queue {}", id_queue.len());

    let handle1 = thread::spawn(move || {
        // Every 30 seconds, try to process something 
        if ! id_queue.is_empty() {
            // Get next value from queue:
            println!("The queue is not empty.");
            let next_value = id_queue.pop_front().unwrap();
            println!("The next value in the queue was {}", next_value);
            thread::sleep(Duration::from_secs(30));
        }
    });
    handles.push(handle1);

    let handle2 = thread::spawn(|| loop {
        let local: DateTime<Local> = chrono::offset::Local::now();
        println!("Current Datetime: {:?}", local);
        thread::sleep(Duration::from_secs(15));
    });
    handles.push(handle2);

    // Finally, let's load all these handles together
    for handle in handles {
        handle.join().unwrap();
    }

    /*
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                thread::spawn(|| handle_client(stream));
            }
            Err(err) => {
                println!("Error: {}", err);
                break;
            }
        }
    }
     */

    std::process::exit(0);
}
