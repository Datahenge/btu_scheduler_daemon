// main.rs

use std::io::Read;

use clap::{App, AppSettings, Arg, SubCommand};  //, ArgMatches, AppSettings;
use serde_json::Value   as SerdeJsonValue;

use btu_scheduler::{
    config::AppConfig,
    rq,
    scheduler,
    task::{BtuTask, print_enabled_tasks},
};


fn add_arguments<'a, 'b>(cli_app: App<'a, 'b>) -> App<'a, 'b> {
    // This function adds arguments and subcommands to a Clap App.

    // Achieving this was trickier than I expected:
    //   1) App has 2 lifetimes, which I had to explicitly name.
    //   2) Methods like arg() and subcommand() take ownership.
    //      So either you chain everything in 1 pass.  Or you use a variable 'ret' to keep capturing ownership.

    // Add some arguments.    
    let ret = cli_app
        .arg(
            Arg::with_name("debug")
            .help("turn on debugging information")
            .short("d")  // a short, one-letter form
        )
        .arg(
            Arg::with_name("config")
            .help("path to configuration file")
            .long("config")
            .takes_value(true)
            .value_name("CONFIG_FILE")
        )
        ;

    // Add some subcommands for Clap.
    let ret = ret
        .subcommand(SubCommand::with_name("test-pickler")
            .about("Call the Frappe web server's BTU 'test-pickler' RPC function.")
        )
        .subcommand(SubCommand::with_name("list-jobs")
            .about("List all known Jobs in the Redis Queue.")
        )
        .subcommand(SubCommand::with_name("list-tasks")
            .about("List all Submitted Tasks stored in the Frappe MariaDB database.")
        )
        .subcommand(SubCommand::with_name("test-ping")
            .about("Call the Frappe web server's BTU 'test_ping' RPC function.")
        )
        .subcommand(SubCommand::with_name("print-config")
            .about("Print the TOML configuration file contents in the terminal.")
        )
        .subcommand(SubCommand::with_name("queue-job-now")
            .about("Queue a Job for immediate execution.")
            .arg(Arg::with_name("job_id")
                .help("the job_id to queue")
                .required(true)
                .takes_value(true)
                .value_name("JOB_ID")
            )
        )
        .subcommand(SubCommand::with_name("queue-task-now")
            .about("Queue a Task for immediate execution.")
            .arg(Arg::with_name("task_id")
                .help("the BTU Task ID to queue")
                .required(true)
                .takes_value(true)
                .value_name("TASK_ID")
            )
        )   
        .subcommand(SubCommand::with_name("show-scheduled")
            .about("Show BTU Tasks that are scheduled in the RQ database.")
        )
        .subcommand(SubCommand::with_name("show-job")
            .about("Show all information about a specific RQ Job.")
			.arg(Arg::with_name("job_id")
				.help("the job_id to examine")
				.required(true)
				.takes_value(true)
				.value_name("JOB_ID")
			)
        )
        ;

    ret
}

fn main() {

	// Step 1.  Create the basic skeleton for the command line application.
    let cli_app = add_arguments(
		App::new("btu-cli")
		.about("CLI for BTU Scheduler")
		.version(btu_scheduler::get_package_version())  // altnerately, .version(crate_version!())
		.author("Brian Pond <brian@datahenge.com>")
		.setting(AppSettings::SubcommandRequiredElseHelp)
	);

	// Note: The method get_matches() takes ownership of a clap App, and returns a ArgMatches.  Effectively destroying App!
	// Having read the Clap comments, apparently this is what the developer intended.
	let matches: clap::ArgMatches = cli_app.get_matches();

	// Step 2.  Load the application configuration.  If CLI was called with --config, pass that argument.
	let app_config: AppConfig;
	match AppConfig::new_from_toml_file(matches.value_of("config")) {
		Ok(result) => {
			app_config = result;
		},
		Err(error) => {
			println!("Error while creating AppConfig from TOML configuration file.\n{}", error);
			std::process::exit(1);
		}
	}

    // Decide if the CLI is running in Debug Mode, or not.
    let debug_mode: bool;
    match matches
        .occurrences_of("debug")
    {
        0 => {
            println!("Debug mode is off");
            debug_mode = false;
        },
        1 => {
            println!("Debug mode is on");
            debug_mode = true;
        },
        _ => {
            println!("Unexpected number of occurrences for debug.");
            debug_mode = true;
        }
    }

	match matches.subcommand() {
		("test-pickler", Some(_)) => {
			cli_btu_test_pickler(&app_config, debug_mode);
		},
		("list-jobs", Some(_)) => {
			cli_list_jobs(&app_config);
		},
		("list-tasks", Some(_)) => {
			cli_list_tasks(&app_config);
		},
		("print-config", Some(_)) => {
			cli_print_config(&app_config);
		},
        ("queue-job-now", Some(arg_matches)) => {
            let job_id: &str = arg_matches.value_of("job_id").unwrap();
			cli_queue_job_immediately(&app_config, job_id);
		},
        ("queue-task-now", Some(arg_matches)) => {
            let task_id: &str = arg_matches.value_of("task_id").unwrap();
			cli_queue_task_immediately(&app_config, task_id);
		},
        ("show-scheduled", Some(_)) => {
			cli_show_scheduled_jobs(&app_config);
		},
		("show-job", Some(arg_matches)) => {
			let job_id: &str = arg_matches.value_of("job_id").unwrap();
			cli_show_job_details(&app_config, job_id);
		},
		("test-ping", Some(_)) => {
			cli_ping_frappe_web(&app_config, debug_mode);
		},
        ("", None) => println!("Please specify a subcommand (stamp, extract)"), // If no subcommand was used it'll match the tuple ("", None)
		_ => unreachable!(), // If all subcommands are defined above, anything else is unreachable!()
	}
}



/*
    The remaining functions below are the "glue" between the CLI and the BTU library.
*/


fn cli_btu_test_pickler(app_config: &AppConfig, debug_mode: bool) {
    /*
        Function calls the Frappe web server, and asks for 'Hello World' in bytes.
    */
    let url: String;
    if app_config.webserver_port == 443 {
        url = format!("https://{}/api/method/btu.btu_api.endpoints.test_function_ping_now_bytes",
                      app_config.webserver_ip);
    }
    else {
        url = format!("http://{}:{}/api/method/btu.btu_api.endpoints.test_function_ping_now_bytes",
                      app_config.webserver_ip, app_config.webserver_port);
    }

    let mut request = ureq::get(&url)
        .set("Authorization", &app_config.webserver_token)
        .set("Content-Type", "application/octet-stream");

    // If Frappe is running via gunicorn, in DNS Multi-tenancy mode, then we have to pass a "Host" header.
    if app_config.webserver_host_header.is_some() {
        request = request.set("Host", &app_config.webserver_host_header.as_ref().unwrap());
    }

    if debug_mode {
        println!("Target URL = {}", url);
        println!("Request = {:?}", request.request_url());
    }

    let resp = request.call().unwrap();

    if debug_mode {
        println!("\nResponse Status = {:?}", resp.status());
        println!("Response = {:?}", resp);
        println!("Response Headers Names = {:?}\n", resp.headers_names());
    }

    assert!(resp.has("content-length"));  // will panic if no Content Length.
    let len = resp.header("content-length")
        .and_then(|s| s.parse::<usize>().ok()).unwrap();

    let mut bytes: Vec<u8> = Vec::with_capacity(len);
    // Read the bytes, up to a maximum:
    resp.into_reader()
        .take(10_000_000)
        .read_to_end(&mut bytes).unwrap();

    assert_eq!(bytes.len(), len);
    println!("HTTP Response as Bytes: {:?}", bytes);
    let bytes_as_string = match std::str::from_utf8(&bytes) {
        Ok(v) => v,
        Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
    };
    println!("HTTP Bytes as UTF-8 String: {}", bytes_as_string);
}


fn cli_list_jobs(app_config: &AppConfig) {
    // Prints all jobs currently stored in Python RQ.
    match rq::get_all_job_ids(app_config) {
        Some(jobs) => {
            if jobs.len() == 0 {
                println!("No jobs were found in Python RQ.");
                return;
            }
            for job in jobs {
                println!("Job: '{}'", job);
            }
        },
        None => {
            println!("No jobs were found in Python RQ.");
        }
    }
}


/**
  Prints to console the ID and Description of all enabled BTU Tasks in the MariaDB database.
*/ 
fn cli_list_tasks(app_config: &AppConfig) {
    print_enabled_tasks(app_config, true);
}


fn cli_ping_frappe_web(app_config: &AppConfig, debug_mode: bool) {
    /*
        Calls a built-in BTU endpoint 'test_ping'
    */
    let url: String;
    if app_config.webserver_port == 443 {
        url = format!("https://{}/api/method/btu.btu_api.endpoints.test_ping",
                      app_config.webserver_ip);
    }
    else {
        url = format!("http://{}:{}/api/method/btu.btu_api.endpoints.test_ping",
                      app_config.webserver_ip, app_config.webserver_port);
    }
    if debug_mode {
        println!("Target URL = {}", url);
    }

    let mut request = ureq::get(&url)
        .set("Authorization", &app_config.webserver_token)
        .set("Content-Type", "application/json");
    // If Frappe is running via gunicorn, in DNS Multi-tenancy mode, then we have to pass a "Host" header.        
    if app_config.webserver_host_header.is_some() {
        request = request.set("Host", &app_config.webserver_host_header.as_ref().unwrap());
    }

    match request.call() {
        Ok(response) => {
            let body = response.into_string().unwrap();
            println!("HTTP Response as String: {}", body);
            let string_as_json: SerdeJsonValue = serde_json::from_str(&body).unwrap();
    
            // Note: The use of 'as_str()' function is because serde's Value automatically displays quotation marks.
            // Converting to an Option<&str> and unwrapping gets rid of them.
            // https://docs.serde.rs/serde_json/#operating-on-untyped-json-values
            let message_value: &str = string_as_json["message"].as_str().unwrap();
            println!("HTTP Response as JSON:  Key 'message' has value '{}'", message_value);
        
        },
        Err(response) => {
            println!("Error:\n{}", response);
        }
    }
		
}


fn cli_print_config(app_config: &AppConfig) {
    println!("{}", app_config);
}


fn cli_queue_job_immediately(app_config: &AppConfig, rq_job_id: &str) -> () {
    // Given an existing RQ Job, push it immediately into Redis Queue.
    if rq::exists_job_by_id(&app_config, &rq_job_id) {
        match rq::enqueue_job_immediate(&app_config, &rq_job_id) {
            Ok(ok_message) => {
                println!("{}", ok_message);
            }
            Err(err_message) => {
                println!("Error while attempting to queue job for execution: {}", err_message);
            }
        }
    }
    else {
        println!("Could not find a job with ID = {}", rq_job_id);
    }
}


fn cli_queue_task_immediately(app_config: &AppConfig, btu_task_id: &str) -> () {
    // 1. Create a Job, based on this Task.
    let task: BtuTask = BtuTask::new_from_mysql(btu_task_id, app_config);
    println!("Fetched task information from SQL: {}", task.task_key);
    println!("------\n{}\n------", task);

    // 2. Create an RQ Job from that Task.
    let rq_job: rq::RQJob = task.to_rq_job(app_config);
    println!("{}\n------", rq_job);

    // 3. Save the new Job into Redis.
    rq_job.save_to_redis(app_config);

    // 4. Enqueue that job for immediate execution.
    match rq::enqueue_job_immediate(&app_config, &rq_job.job_key_short) {
        Ok(ok_message) => {
            println!("Successfully enqueued: {}", ok_message);
        }
        Err(err_message) => {
            println!("Error while attempting to queue job for execution: {}", err_message);
        }
    }

    ()
}


fn cli_show_job_details(app_config: &AppConfig, job_id: &str) -> () {
	// println!("Attempting to fetch information about Job with ID = {}", job_id);
    match rq::read_job_by_id(app_config, job_id) {
        Ok(ok_message) => {
            println!("{}", ok_message);
        }
        Err(err_message) => {
            println!("{}", err_message);
        }
    }
}


fn cli_show_scheduled_jobs(app_config: &AppConfig) {
	scheduler::rq_print_scheduled_tasks(app_config, true);
}
