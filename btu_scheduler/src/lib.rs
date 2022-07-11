#![forbid(unsafe_code)]
#![allow(dead_code)]
#![allow(unused_imports)]

// Main library 'btu_scheduler'
// These modules are located in adjacent files.
use mysql::PooledConn;
use mysql::prelude::Queryable;
use serde::Deserialize;

pub mod btu_cron;
pub mod config;
pub mod errors;
pub mod logging;
pub mod rq;
pub mod scheduler;

#[cfg(feature = "email")]
pub mod email;

mod tests;
use crate::config::AppConfig;

// This is the response from an HTTP call to Frappe REST API.
#[derive(Deserialize, Debug)]
struct FrappeApiMessage {
	message: Vec<u8>
}

pub fn get_package_version() -> &'static str {
    // Completed.
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    VERSION
}


pub mod task {
	
	use std::fmt;
	use mysql::prelude::Queryable;
	use mysql::PooledConn;
	use tracing::{trace, debug, info, warn, error, span, Level};
	use crate::config::{self, AppConfig};
	use crate::rq::RQJob;
	
	pub struct BtuTask {
		pub task_key: String,
		desc_short: String,
		desc_long: String,
		arguments: Option<String>,
		path_to_function: String,	// example:  btu.manual_tests.ping_with_wait
		pub max_task_duration: u32,  // example:  600
	}

	// TODO: Need to resolve SQL injection possibility.  Probably means crabbing some more Crates.
	impl BtuTask {

		pub fn new_from_mysql(task_key: &str, app_config: &AppConfig) -> Self {
			let mut sql_conn: PooledConn = config::get_mysql_conn(app_config).unwrap();

			let query_syntax = format!("SELECT name AS task_key, desc_short, desc_long,
			arguments, function_string AS path_to_function,	max_task_duration 
			FROM `tabBTU Task` WHERE name = '{}' LIMIT 1;", task_key);

			// OPTION 1: Working 1 row at a time.
			/*
			let row: mysql::Row = sql_conn.query_first(&query_syntax).unwrap().unwrap();
			info!("mysql Row named foo = {:?}", row);

			let mut task: BtuTask = BtuTask::default();
			task.task_key = row.get(0).unwrap();
			// Short Description
			if let Some(row_outer) = row.get_opt(1) {
				if let Ok(row_inner) = row_outer {
					task.desc_short = row_inner;
				}
			}
			// Long Description
			if let Some(row_outer) = row.get_opt(2) {
				if let Ok(row_inner) = row_outer {
					task.desc_long = row_inner;
				}
			}
			//task.arguments =  row.get(3).unwrap();
			//task.path_to_function = row.get(4).unwrap();
			//task.max_task_duration = row.get(5).unwrap();
			*/

			/*
				Option 2:  Using a map.
				NOTE: The use of 'get_opt()' is necessary to handle SQL rows containing NULLs, instead of the expected datatype.
			*/
			let task: BtuTask = sql_conn.query_first(query_syntax).unwrap().map(|row: mysql::Row| {
					BtuTask {
						task_key: row.get(0).unwrap(),
						desc_short: row.get_opt(1).unwrap_or(Ok("".to_owned())).unwrap_or("".to_owned()),
						desc_long: row.get_opt(2).unwrap_or(Ok("".to_owned())).unwrap_or("".to_owned()),
						arguments: row.get_opt(3).unwrap_or(Ok(None)).unwrap_or(None),
						path_to_function:  row.get(4).unwrap_or("".to_owned()),
						max_task_duration: row.get_opt(5).unwrap_or(Ok(600)).unwrap_or(600),
					}
				}).unwrap();
			info!("{}", task);
			task
		}

		/// Create an RQ Job struct from a BTU Task Schedule struct.
		pub fn to_rq_job(&self, app_config: &AppConfig) -> RQJob {

			let mut new_job: RQJob = RQJob::new_with_defaults();
			new_job.description = self.desc_short.clone();
			match crate::get_pickled_function_from_web(&self.task_key, None, app_config) {
				Ok(byte_result) => {
					new_job.data = byte_result;
				}
				Err(error_message) => {
					panic!("Error while requesting pickled Python function:\n{}", error_message);
				}
			}
			new_job.timeout = self.max_task_duration;
			new_job
		}


	}

	impl fmt::Display for BtuTask {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			// This syntax helpfully ignores the leading whitespace on successive lines.
			write!(f,  "task_key: {}\n\
						desc_short: {}\n\
						desc_long: {}\n\
						arguments: {:?}\n\
						path_to_function: {}\n\
						max_task_duration: {}",
				self.task_key, self.desc_short,
				self.desc_long, self.arguments, self.path_to_function, self.max_task_duration
			)
		}
	}

	pub fn print_enabled_tasks(app_config: &AppConfig, to_stdout: bool) -> () {

		let mut sql_conn: PooledConn;
		match config::get_mysql_conn(app_config) {
			Ok(_conn) => {
				sql_conn = _conn;
			},
			Err(err) => {
				error!("Error while attempting to get connection in 'query_task_summary' : {}", err);
				return ()
			}
		}

		let query_syntax = "SELECT name, desc_short	FROM `tabBTU Task` WHERE docstatus = 1 AND is_transient = 0";
		let task_vector: Vec<(String,String)> = sql_conn.query_map(query_syntax, |row: mysql::Row| {
			(row.get(0).unwrap(), row.get(1).unwrap())
		}).unwrap();

		// TODO: Create a new macro that combines info! and println!, or warn! and println, etc.
		// Something like echo!(level, message, to_stdout) ?
		if task_vector.len() > 0 {
			for task in task_vector {
				if to_stdout {
					println!("Task {} : {}", task.0, task.1);
				}
				else {
					info!("Task {} : {}", task.0, task.1);
				}
			}
		}
		else {
			if to_stdout { 
				println!("No BTU Tasks are defined in the MariaDB database.");
			}
			else {
				warn!("No BTU Tasks are defined in the MariaDB database.");
			}

		}
	}
}  // end of task module.

pub mod task_schedule {
	
	use std::convert::TryFrom;
	use anyhow::anyhow as anyhow_macro;
	use chrono::{DateTime, Utc};
	use chrono_tz::Tz;
	use mysql::PooledConn;
	use mysql::prelude::Queryable;
	use tracing::{trace, debug, info, warn, error, span, Level};
	use crate::btu_cron;
	use crate::config::{self, AppConfig};
	use crate::rq::RQJob;
	use crate::task::BtuTask;
	use crate::scheduler::RQScheduledTask;

	// Newtype Pattern:
	pub struct MyTz(Tz);
	
	impl TryFrom<String> for MyTz {
		type Error = String;
		fn try_from(any_string: String) -> Result<Self, Self::Error> {
			let _: Tz = match any_string.parse() {
				Ok(v) => {
					return Ok(MyTz(v));
				}
				Err(_) => {
					let new_error = format!("Cannot convert string '{}' to a chrono Time Zone.", any_string);
					return Err(new_error);
				}
			};
		}
	}

	// Deliberately excluding SQL columns that don't matter for this program.
	#[derive(Debug, Clone)]
	pub struct BtuTaskSchedule {
		pub id: String,
		task: String,
		task_description: String,
		pub enabled: u8,
		queue_name: String,
		redis_job_id: Option<String>,  // Using Option here, because it's quite possible for BTU App to create a schedule, but not populate this!
		argument_overrides: Option<String>,  // MUST use Option here, if the result is at all Nullable.
		schedule_description: String,
		pub cron_string: String,
		pub cron_timezone: chrono_tz::Tz
	}

	impl BtuTaskSchedule {
		/**
			Create a new BtuTask struct by reading from the MariaDB database.
		*/
		pub fn build_task_from_database(&self, app_config: &config::AppConfig) -> crate::task::BtuTask {
			let task: BtuTask = BtuTask::new_from_mysql(&self.task, app_config);
			task
		}

		/// Create an RQ Job struct from a BTU Task Schedule struct.
		pub fn to_rq_job(&self, app_config: &AppConfig) -> Result<RQJob, anyhow::Error> {

			let mut new_job: RQJob = RQJob::new_with_defaults();
			new_job.description = self.task_description.clone();

			match crate::get_pickled_function_from_web(&self.task, Some(&self.id), app_config) {
				Ok(byte_result) => {
					new_job.data = byte_result;
				}
				Err(error_message) => {
					// without the turbofish, I get a "type annotations needed" warning from the compiler.
					return Err::<RQJob, anyhow::Error>(anyhow_macro!("Error while requesting pickled Python function:\n{}", error_message));
				}
			}
			Ok(new_job)
		}

		/**
			Return on optional Vector of UTC Datetimes, which are the next execution times for this Task Schedule.
		 */
		pub fn next_runtimes(&self, from_utc_datetime: &Option<DateTime<Utc>>, number_results: &usize) -> Option<Vec<DateTime<Utc>>> {

			let next_runtimes = btu_cron::tz_cron_to_utc_datetimes(
				&self.cron_string,
				self.cron_timezone,
				*from_utc_datetime,
				number_results
			);

			if next_runtimes.is_err() {
				error!("Cannot calculate 'Next Execution Time' values for Task Schedule {}", &self.id);
				return None;
			}				
			if next_runtimes.as_ref().unwrap().len() == 0 {  // error because no results were returned
				error!("Cannot calculate 'Next Execution Time' values for Task Schedule {}", &self.id);
				return None;
			}

			Some(next_runtimes.unwrap())
			// let result: Vec<DateTime<Utc>> = next_runtimes.unwrap();
			// Some(result)
		}
	}

	/** Given a Task Schedule identifier (string), connect to MySQL, query the table,
	    and return a new instance of struct 'BtuTaskSchedule'.
	*/
	pub fn read_btu_task_schedule(app_config: &config::AppConfig, task_schedule_id: &str) -> Option<BtuTaskSchedule> {

		let mut sql_conn: PooledConn = config::get_mysql_conn(&app_config).unwrap();  // create a connection to the MariaDB database.

		// 2. Run query, and map result into a new Result<Option<BtuTaskSchedule>>
		//    TODO: Investigate resolving SQL injection.  Probably means finding a helpful 3rd party crate.
		let query_syntax = format!("SELECT TaskSchedule.name, TaskSchedule.task, TaskSchedule.task_description,
		TaskSchedule.enabled, TaskSchedule.queue_name, TaskSchedule.redis_job_id, TaskSchedule.argument_overrides,
		TaskSchedule.schedule_description, TaskSchedule.cron_string, Configuration.value AS cron_time_zone

		FROM `tabBTU Task Schedule` AS TaskSchedule

		INNER JOIN `tabSingles`	AS Configuration
		ON Configuration.doctype = 'BTU Configuration'
		AND Configuration.`field` = 'cron_time_zone'
		
		WHERE TaskSchedule.name = '{}' LIMIT 1;", task_schedule_id);

		/* TODO: exec_map appears entirely broken.
			thread '<unnamed>' panicked at 'Could not retrieve alloc::string::String from Value', 
			/home/sysop/.cargo/registry/src/github.com-1ecc6299db9ec823/mysql_common-0.27.5/src/value/convert/mod.rs:175:23
		*/

		// TODO: Error handling if the query fails.
		let result_task_schedules: Result<Vec<BtuTaskSchedule>, mysql::Error> = sql_conn
			.query_map(query_syntax, |row: mysql::Row| {
				BtuTaskSchedule {
					id:  row.get(0).unwrap(),
					task:row.get(1).unwrap(),
					task_description: row.get(2).unwrap(),
					enabled:  row.get(3).unwrap(),
					queue_name:  row.get(4).unwrap(),
					redis_job_id:  row.get(5).unwrap(),
					argument_overrides: row.get(6).unwrap(),
					schedule_description:row.get(7).unwrap(),
					cron_string:  row.get(8).unwrap(),
					cron_timezone: row.get::<String, _>(9).unwrap().parse().unwrap()
				}
			});

		let task_schedules: Vec<BtuTaskSchedule>;  // uninitialized until match below -->
		match result_task_schedules {
			Ok(result) => {
				task_schedules = result;
			}
			Err(mysql_error) => {
				error!("MySQL Error encountered in read_btu_task_schedule(): {:?}", mysql_error);
				return None;
			}
		}

  		// The SQL query returns 0 or 1 rows.  The syntax below uses 'next()' to fetch the first element in the Vector.
		if let Some(btu_task_schedule) =  task_schedules.iter().next() {
			Some(btu_task_schedule.to_owned())
		} else {
			// No results returned from SQL query.
			error!("Cannot find a record in 'tabBTU Task Schedule' with primary key '{}'", task_schedule_id);
			None
		}       
	}
}


/// Call ERPNext REST API and acquire pickled Python function as bytes.
fn get_pickled_function_from_web(task_id: &str, task_schedule_id: Option<&str>, app_config: &AppConfig) -> Result<Vec<u8>, String> {

	let url: String = format!("http://{}:{}/api/method/btu.btu_api.endpoints.get_pickled_task",
		app_config.webserver_ip, app_config.webserver_port);

	let mut request = ureq::get(&url)
		.set("Authorization", &app_config.webserver_token)
		.set("Content-Type", "application/json");  // Using json, because that's what we're sending 'task_id' as below.

		// If Frappe is running via gunicorn, in DNS Multi-tenancy mode, then we have to pass a "Host" header.
    if app_config.webserver_host_header.is_some() {
        request = request.set("Host", &app_config.webserver_host_header.as_ref().unwrap());
    }

	let wrapped_response = request
		.send_json(ureq::json!({
			"task_id": task_id,
			"task_schedule_id": task_schedule_id
		}));

	if wrapped_response.is_err() {
		return Err(format!("Error in response: {:?}", wrapped_response.err()));
	}

	let web_server_resp = wrapped_response.unwrap()
	;
	assert!(web_server_resp.has("Content-Length"));
	let _response_length = web_server_resp.header("Content-Length")
		.and_then(|s| s.parse::<usize>().ok()).unwrap();

	// Store the response in a FrappeApiMessage struct.
	let response_json: FrappeApiMessage = web_server_resp.into_json().unwrap();
	let bytes: Vec<u8> = response_json.message;
	return Ok(bytes);
}


/**

  Validates the SQL connection by performing a simple query against SQL table 'tabDocType'
*/
pub fn validate_sql_credentials(app_config: &config::AppConfig) -> Result<(), std::io::Error> {

	let sql_conn: Result<PooledConn, mysql::Error> = config::get_mysql_conn(&app_config);
	if sql_conn.is_err() {
		let sql_error_string: String = sql_conn.err().unwrap().to_string();
		let io_error = std::io::Error::new(std::io::ErrorKind::Other, sql_error_string);
		return Err(io_error);
	}
	let mut sql_conn: PooledConn = sql_conn.unwrap();  // create a connection to the MariaDB database.

	// 2. Run query, and map result into a new Result<Option<BtuTaskSchedule>>
	//    TODO: Investigate resolving SQL injection.  Probably means finding a helpful 3rd party crate.
	let query_string: &'static str = "SELECT count(*) FROM tabDocType;";

	let query_result: Result<Option<u64>, mysql::Error> = sql_conn.query_first(query_string);
	match query_result {
		Ok(result_option) => {
			if result_option.is_none() {
				// Return an Error if there are no results.
				let io_error = std::io::Error::new(std::io::ErrorKind::Other, format!("Query of DocType table returned no results."));
				return Err(io_error);				
			}
			let number_of_doctypes: u64 = result_option.unwrap();
			if number_of_doctypes == 0 {
				// Return an Error if SQL table `tabDocType` has no zero rows (unlikely condition, but worth checking)
				let io_error = std::io::Error::new(std::io::ErrorKind::Other, format!("Query of DocType table returned 0 rows."));
				return Err(io_error);
			}
		},
		Err(error) => {
			let io_error = std::io::Error::new(std::io::ErrorKind::Other, error);
			return Err(io_error);
		}
	}

	Ok(())
}

#[allow(dead_code)]
fn sorted_vector_of_kv(kv_pairs: &Vec<(String, String)>) -> Vec<(String, String)> {

    let mut my_vector : Vec<(String, String)> = Vec::new();

    // Loop through an iterator of all the Environment Variables (includes any .env files)
    for (key, value) in kv_pairs {
        my_vector.push(
			(key.to_owned(), value.to_owned())
		);
    }
    // Sort by the first item in the tuple pair.
    my_vector.sort_by(|foo, bar| foo.0.partial_cmp(&bar.0).unwrap_or(std::cmp::Ordering::Equal));

    // Return the sorted vector of values.
    my_vector
}
