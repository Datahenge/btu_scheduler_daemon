use std::{collections::VecDeque, io::{BufRead, BufReader, Read, Write}, os::unix::net::{UnixStream, UnixListener}, str::Bytes, sync::{Arc, Mutex}};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct FrappeClientMessage {
    request_type: String,
    request_content: Option<String>
}

/**
Given a String representing the path to to a socket file, create a "listener" for that path.
*/
pub fn create_socket_listener(socket_file_path: &str) -> UnixListener {

    let file_as_path: Utf8PathBuf = socket_file_path.into();
    if file_as_path.exists() {
        // delete the socket file, if it exists:
        std::fs::remove_file(&file_as_path) // Pass a reference, so we don't lose ownership.
            .expect(&format!("ERROR: Could not remove file '{}'", file_as_path));
    }
    let listener = UnixListener::bind(&file_as_path).unwrap();  // NOTE: This is a MOVE of 'socket_path'.
    println!("{} Listening for inbound traffic on Unix Domain Socket '{}'", '\u{2713}', file_as_path);
    return listener;
}


pub fn handle_client_request(mut stream: UnixStream, 
                             _queue: Arc<Mutex<VecDeque<std::string::String>>>) -> Result<String,std::io::Error> {

    /*
        Part 1:  Read some bytes.

        Developers take note: there are MANY wrong ways to implement the code below.  None of which will create compiler errors.

        * Reading too few bytes.  For example, create buffer as Vec::new() instead of a fixed length.
        * Storing extra, empty bytes.  For example, by creating buffer as vec![0; 512]; or [0; 4096];
        * Using 'stream.read_to_string()' or 'stream.read_to_end()'.  These expect an EOF that will never arrive, so the client Times Out.

        For the moment, I'm knowingly doing a Wrong Thing, because I don't have time to build the Right Thing.
        1.  I'm creating a vector of 1k bytes.
        2.  I'm reading what Python sends me.  (NOTE: If you try read_to_end() Python never thinks you finished reading, and times out.)
        3.  The end of the 1k bytes are filled with 0's
        4.  I strip them out.
        5.  I now have a perfectly formed JSON string, which can be matched to a FrappeClientMessage struct.

        TODO:
        * Create a vector with capacity.
        * Read only the bytes that are sent.
        * Reply smartly to Python so it doesn't Time Out.
    */

    let mut buffer = [0; 1024];
    stream.read(&mut buffer)?;
    // dbg!("Buffer has length: {}", buffer.len());
    let mut buffer_as_string = match std::str::from_utf8(&buffer) {
        Ok(v) => v,
        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
    };
    buffer_as_string = buffer_as_string.trim_matches(char::from(0));  // remove all the training zero's
    println!("Buffer as string: {}", buffer_as_string);

    // Part 2: Response varies with request:
    let client_message: Result<FrappeClientMessage, serde_json::Error> = serde_json::from_str(&buffer_as_string);

    // If message from socket client cannot be coerced into a FrappeClientMessage:
    if client_message.is_err() {
        let error_string: String = client_message.unwrap_err().to_string();
        println!("Error while parsing client message: {}", &error_string);
        let new_error = std::io::Error::new(std::io::ErrorKind::Other, error_string);
        return Err(new_error);  // if cannot coerce into FrappeClientMessage, return an error String.
    }

    // Action and Response varies depending on the 'request_type'
    let client_message = client_message.unwrap();  // overshadow the original variable with the unwrapped contents.
    match client_message.request_type.as_str() {
        "ping" => {
            println!("Client sent a 'ping' ...");
            let mut stream_out = stream.try_clone()?;
            stream_out.write_all("pong".as_bytes()).expect("Failed to 'write_all'");
            println!("...replied back with 'pong'");
            return Ok("Replied to client's 'ping' with a 'pong'".to_owned())
        },
        _ => {
            let error_string: String =  format!("Client message has an unhandled 'request_type': {}", client_message.request_type);
            println!("{}", error_string);
            let new_error = std::io::Error::new(std::io::ErrorKind::Other, error_string);
            return Err(new_error);  // if cannot coerce into FrappeClientMessage, return an error String.
        }
    }

    /*
        let task_schedule_id = line.unwrap();
        println!("Adding value {} to the internal queue.", task_schedule_id);
        // Wait until last possible moment to obtain lock, then drop it.
        if let Ok(mut unlocked_queue) = queue.lock() {
            unlocked_queue.push_back(task_schedule_id);
        }
        else {
            println!("Error in 'handle_socket_client' while attempting to unlock internal queue.");
        }
    */
}

/*
    Known-to-be-good function for reading the Unix Domain Socket client data.
*/
#[allow(unused_must_use)]
pub fn known_good_example(mut stream: UnixStream, 
    _queue: Arc<Mutex<VecDeque<std::string::String>>>) -> Result<String,std::io::Error> {

    println!("Reading from stream...");
    let mut buffer: Vec<u8> = Vec::new();
    stream.read(&mut buffer);
    let mut stream_out = stream.try_clone()?;
    stream_out.write_all("pong".as_bytes()).expect("Failed to 'write_all'");

    return Ok("".to_owned())
}
