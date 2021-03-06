/* ipc_stream.rs */

#![forbid(unsafe_code)]

// This module handles Inter-process Communication with the colocated Frappe Web Server.

use std::{collections::VecDeque, io::{Read, Write},
          os::unix::net::{UnixStream, UnixListener},
          sync::{Arc, Mutex}};

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use tracing::{trace, debug, info, warn, error, span, Level};
use crate::config;
use crate::scheduler::rq_cancel_scheduled_task;

#[derive(Serialize, Deserialize, Debug)]
struct FrappeClientMessage {
    request_type: String,
    request_content: Option<String>
}

/**
Create a UnixListener using a string slice, where the slice is a path to a Unix Domain Socket file.
*/
pub fn create_socket_listener(socket_file_path: &str) -> UnixListener {

    let file_as_path: Utf8PathBuf = socket_file_path.into();
    if file_as_path.exists() {
        // Delete any pre-existing socket file:
        std::fs::remove_file(&file_as_path) // Pass a reference, so we don't lose ownership.
            .expect(&format!("ERROR: On deamon startup, could not remove preexisting socket file '{}'", file_as_path));
    }
    let listener = UnixListener::bind(&file_as_path).unwrap();
    info!("Listening for inbound traffic on Unix Domain Socket '{}'", file_as_path);
    return listener;
}

/** 
  This function grants full permission to a Socket File for a Linux Group name.

  On Linux, when you create a socket file, the file's permissions are NOT inherited from the containing directory.
  Instead, the owner of the socket file (the account that created it) has full permissions.  Group and Other do not.
 */
pub fn update_socket_file_permissions(socket_file_path: &str, unix_group_name: &str) -> std::io::Result<()> {

    use std::os::unix::fs::PermissionsExt;
    use nix::unistd::Group as LinuxGroup;

    // 1. Find the unique group id (gid) for the desired Unix Group name.
    let unix_group: Option<LinuxGroup> = LinuxGroup::from_name(unix_group_name)?;
    if unix_group.is_none() {
        let this_error = std::io::Error::new(std::io::ErrorKind::Other, format!("Unable to find a Linux Group with name = '{}'", unix_group_name));
        return Err(this_error);
    }
    let unix_group = unix_group.unwrap();  // unwrap and shadow the original variable.
    let unix_group_uid = unix_group.gid;

    // 2. Change the socket file's group owner, to that group.
    nix::unistd::chown(socket_file_path, None, Some(unix_group_uid))?;

    // 3. Update the socket file's permissions, granting Read, Write, and Execute to the Linux user group (passed via argument)
    let mut permissions = std::fs::metadata(socket_file_path)?.permissions();
    permissions.set_mode(0o140775); // set permissions to 775 (rwxrwxr-x)
    /* 
        IMPORTANT: The code above does NOT modify the file.  Just an in-memory Permissions struct.
        The line below is required to update the disk:
    */
    std::fs::set_permissions(socket_file_path, permissions)?;

    // Sanity check: Re-read the permissions from disk and compare.
    /*
    let permissions = std::fs::metadata(socket_file_path)?.permissions();
    assert_eq!(
        format!("{:o}", permissions.mode()),
        "140775"
    );
     */
    Ok(())
}


pub fn handle_client_request(mut stream: UnixStream, 
                             queue: Arc<Mutex<VecDeque<std::string::String>>>,
                             app_config: &config::AppConfig) -> Result<String,std::io::Error> {

    /*
        Part One:  Read bytes from a socket Client.

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

    // Part 2: Response varies with request:
    let client_message: Result<FrappeClientMessage, serde_json::Error> = serde_json::from_str(&buffer_as_string);

    // If message from socket client cannot be coerced into a FrappeClientMessage:
    if client_message.is_err() {
        let error_string: String = client_message.unwrap_err().to_string();
        error!("Error while parsing client message: {}", &error_string);
        let new_error = std::io::Error::new(std::io::ErrorKind::Other, error_string);
        return Err(new_error);  // if cannot coerce into FrappeClientMessage, return an error String.
    }

    // Action and Response varies depending on the 'request_type'
    let client_message = client_message.unwrap();  // overshadow the original variable with the unwrapped contents.
    match client_message.request_type.as_str() {
        "ping" => {
            info!("Frappe Web Server sent a 'ping' request ...");
            let mut stream_out = stream.try_clone()?;
            stream_out.write_all("pong".as_bytes()).expect("Failed to 'write_all'");
            info!("...replied back with 'pong'");
            return Ok("Replied to client's 'ping' with a 'pong'".to_owned())
        },
        "create_task_schedule" => {
            // This request must have arrive with a 2nd argument: 'request_content'
            if client_message.request_content.is_none() {
                let new_error = std::io::Error::new(std::io::ErrorKind::Other, "Request 'build_task_schedule' missing required argument 'request_content'");
                return Err(new_error);
            }
            let task_schedule_id = client_message.request_content.unwrap();
            info!("Frappe Web Server requesting Task Schedule '{}' be processed for Python RQ.  Adding this to the Scheduler's internal queue.", task_schedule_id);

            // Wait until last possible moment to obtain lock on internal queue.  Drop immediately when done.
            if let Ok(mut unlocked_queue) = queue.lock() {
                unlocked_queue.push_back(task_schedule_id.clone());  // VecDequeue takes ownership forever; need to clone here to continue using 'task_schedule_id'
            }
            else {
                let new_error = std::io::Error::new(std::io::ErrorKind::Other, "Error in function 'handle_client_request' while attempting to unlock internal queue.");
                return Err(new_error);
            }
            // Reply back to Unix Domain Socket client:
            let mut stream_out = stream.try_clone()?;
            stream_out.write_all(format!("BTU Scheduler now re-processing Task Schedule {} in Python RQ.",task_schedule_id)
                .as_bytes()).expect("Failed to 'write_all'");
            return Ok("Replied successfully to UDS client's 'build_task_schedule' request.".to_owned())
        },
        "cancel_task_schedule" => {
            // This request must have arrive with a 2nd argument: 'request_content', which is the Task Schedule ID.
            if client_message.request_content.is_none() {
                let new_error = std::io::Error::new(std::io::ErrorKind::Other, "Request 'cancel_task_schedule' missing required argument 'request_content'");
                return Err(new_error);
            }
            let task_schedule_id = client_message.request_content.unwrap();
            info!("Frappe Web Server requesting Task Schedule '{}' be cancelled in Python RQ.", task_schedule_id);

            let mut stream_out = stream.try_clone()?;
            // Try to cancel, and reply back to the UDS Client:
            match rq_cancel_scheduled_task(app_config, &task_schedule_id) {
                Ok(_) => {
                    let okay_message: String = format!("Successfully cancelled BTU Task Schedule {} in Python RQ.",task_schedule_id);
                    info!("{}", okay_message);
                    stream_out.write_all(okay_message.as_bytes()).expect("Failed to 'write_all'");

                    // Before finishing, log the Tasks that are still known to the BTU:
                    crate::scheduler::rq_print_scheduled_tasks(&app_config, false);      
                    return Ok(okay_message)
                },
                Err(error_message) => {
                    stream_out.write_all(error_message.as_bytes()).expect("Failed to 'write_all'");
                    let new_error = std::io::Error::new(std::io::ErrorKind::Other, error_message);
                    return Err(new_error);
                }
            }
        },

        _ => {
            // No match for the 'request_type'
            let error_string: String =  format!("Client message has an unhandled 'request_type': {}", client_message.request_type);
            let mut stream_out = stream.try_clone()?;
            // 1. Return an message over the UDS to the client:
            stream_out.write_all(error_string.as_bytes()).expect("Failed to 'write_all'"); // Return this error to the caller
            // 2. Print the same error message to stdout
            error!("{}", error_string);
            // 3. Return the error upward
            let new_error = std::io::Error::new(std::io::ErrorKind::Other, error_string);
            return Err(new_error);
        }
    }
}

/*
    Known-to-be-good function for reading the Unix Domain Socket client data.

#[allow(unused_must_use)]
pub fn known_good_example(mut stream: UnixStream, 
    _queue: Arc<Mutex<VecDeque<std::string::String>>>) -> Result<String,std::io::Error> {

    info!("Reading from stream...");
    let mut buffer: Vec<u8> = Vec::new();
    stream.read(&mut buffer);
    let mut stream_out = stream.try_clone()?;
    stream_out.write_all("pong".as_bytes()).expect("Failed to 'write_all'");

    return Ok("".to_owned())
}

*/
