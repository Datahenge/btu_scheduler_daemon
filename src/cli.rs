// Third Party

extern crate clap;

use clap::{App, AppSettings, Arg, SubCommand};  //, ArgMatches, AppSettings;
use pyrq_scheduler::config::AppConfig;
use pyrq_scheduler::task_scheduler;
use pyrq_scheduler::pyrq;

fn cli_show_scheduled_jobs(app_config: &AppConfig) {
	task_scheduler::rq_print_scheduled_tasks(app_config);
}

fn cli_show_job_details(app_config: &AppConfig, job_id: &str) {
	pyrq::read_job_by_id(app_config, job_id);
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
                .help("The identifier of the job in RQ.")
                .required(true)
            )
        );
    ret
}