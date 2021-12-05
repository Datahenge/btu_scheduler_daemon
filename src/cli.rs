// Third Party

extern crate clap;

use std::io::Read;
use clap::{App, AppSettings, Arg, SubCommand};  //, ArgMatches, AppSettings;
use serde_json::Value   as SerdeJsonValue;

use pyrq_scheduler::config::AppConfig;
use pyrq_scheduler::task_scheduler;
use pyrq_scheduler::pyrq;

fn cli_show_scheduled_jobs(app_config: &AppConfig) {
	task_scheduler::rq_print_scheduled_tasks(app_config);
}

fn cli_show_job_details(app_config: &AppConfig, job_id: &str) -> () {
	println!("Attempting to fetch information about Job with ID = {}", job_id);
	pyrq::read_job_by_id(app_config, job_id);
}

fn cli_ping_frappe_web(app_config: &AppConfig) {
    let url: String = format!("http://{}:{}/api/method/btu.btu_api.endpoints.ping_from_caller",
        app_config.webserver_ip, app_config.webserver_port);
    // println!("Calling URL: {}", url);
    let body: String = ureq::get(&url)
        .set("Authorization", &app_config.webserver_token)
        .set("Content-Type", "application/json")
		.call().unwrap()
		.into_string().unwrap();
    println!("HTTP Response as String: {}", body);

    let string_as_json: SerdeJsonValue = serde_json::from_str(&body).unwrap();
    
    // Note: The use of 'as_str()' function is because serde's Value automatically displays quotation marks.
    // Converting to an Option<&str> and unwrapping gets rid of them.
    // https://docs.serde.rs/serde_json/#operating-on-untyped-json-values
    let message_value: &str = string_as_json["message"].as_str().unwrap();
    println!("HTTP Response as JSON:  Key 'message' has value '{}'", message_value);
}

fn cli_bytes_frappe_web(app_config: &AppConfig) {
    // Function calls the Frappe web server, and asks for 'Hello World' in bytes.
    let url: String = format!("http://{}:{}/api/method/btu.btu_api.endpoints.bytes_from_caller",
        app_config.webserver_ip, app_config.webserver_port);

    let resp = ureq::get(&url)
        .set("Authorization", &app_config.webserver_token)
        .set("Content-Type", "application/octet-stream")
		.call().unwrap();

    assert!(resp.has("Content-Length"));
    let len = resp.header("Content-Length")
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


fn main() {

	// Step 1.  Load the application configuration.
	let app_config: AppConfig;
	match AppConfig::new_from_toml_file() {
		Ok(result) => {
			app_config = result;
		},
		Err(error) => {
			println!("Error while creating AppConfig from TOML configuration file.\n{}", error);
			std::process::exit(1);
		}
	}

	// Step 2.  Create the basic skeleton for the command line application.
	//			Note: The 'add_arguments" function is defined further below.
	let cli_app = add_arguments(
		App::new("btu-cli")
		.about("CLI for BTU Scheduler")
		.version(pyrq_scheduler::get_package_version())  // altnerately, .version(crate_version!())
		.author("Brian Pond <brian@datahenge.com>")
		.setting(AppSettings::SubcommandRequiredElseHelp)
	);

	// Warning: The method get_matches() takes ownership of a clap App, and returns a ArgMatches.  Effectively destroying App!
	// Having read the Clap comments, apparently this is what the developer intended.
	let matches = cli_app.get_matches();

	match matches.subcommand() {
		("show-scheduled", Some(_)) => {
			cli_show_scheduled_jobs(&app_config);
		},
		("show-job", Some(arg_matches)) => {
			let job_id: &str = arg_matches.value_of("job_id").unwrap();
			cli_show_job_details(&app_config, job_id);
		},
		("ping-webserver", Some(_)) => {
			cli_ping_frappe_web(&app_config);
		},
		("bytes-webserver", Some(_)) => {
			cli_bytes_frappe_web(&app_config);
		},
		("", None) => println!("Please specify a subcommand (stamp, extract)"), // If no subcommand was used it'll match the tuple ("", None)
		_ => unreachable!(), // If all subcommands are defined above, anything else is unreachable!()
	}
}

fn add_arguments<'a, 'b>(cli_app: App<'a, 'b>) -> App<'a, 'b> {
    // This function adds arguments and subcommands to a Clap App.

    // Achieving this was trickier than I expected:
    //   1) App has 2 lifetimes, which I had to explicitly name.
    //   2) Methods like arg() and subcommand() take ownership.
    //      So either you chain everything in 1 pass.  Or you use a variable let 'ret' to keep capturing ownership.

    // Add some arguments.    
    let ret = cli_app
        .arg(
            Arg::with_name("debug")
            .help("turn on debugging information")
            .short("d")
        );

    // Add some subcommands for Clap.
    let ret = ret
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
        .subcommand(SubCommand::with_name("ping-webserver")
            .about("Call the Frappe web server's BTU 'ping_from_caller' RPC function.")
        )
        .subcommand(SubCommand::with_name("bytes-webserver")
            .about("Call the Frappe web server's BTU 'bytes_from_caller' RPC function.")
        );
    ret
}
