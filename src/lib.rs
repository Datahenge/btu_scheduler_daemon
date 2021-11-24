#![forbid(unsafe_code)]

pub mod error {
	use toml::de::Error		as TomlError;
	use thiserror::Error	as ThisError;

	#[derive(ThisError, Debug)]
	pub enum ConfigError {
		#[error("Could not deserialize TOML into a Rust object.\n    {source:?}")]
		ConfigLoad {
			#[from] 
			source: TomlError,
		},
		#[error("Cannot find the TOML configuration file on disk.")]
		MissingConfigFile,
	}
}

pub mod config {
	
	use crate::error::ConfigError;
	use std::{fmt, fs};
	use std::path::Path;
	use serde::{Deserialize};  // Also there is Serialize
	// use mysql::*;			// DO NOT import mysql like this, because it overrides the default Error type.
	use mysql::{Opts, Pool};

	#[derive(Deserialize)]
	pub struct AppConfig {
		pub max_seconds_between_updates: u32,
		mysql_user: String,
		mysql_password: String,
		mysql_host: String,
		mysql_port: Option<u32>,
		mysql_database: String,
		pub rq_host: String,
		pub rq_port: u32
	}

	impl AppConfig {
		pub fn new_from_toml_string(any_string: &str) -> Result<AppConfig, ConfigError> {

			match toml::from_str(&any_string) {
				Ok(app_config) => {
					Ok(app_config)
				},
				Err(error) => {
					return Err(ConfigError::ConfigLoad { source: error });
				}
			}

		}
	}

	impl AppConfig {
    	// Associated function signature; `Self` refers to the implementor type.
		pub fn new_from_toml_file() -> Result<AppConfig, ConfigError> {

			// Read TOML file, and store values here in this configuration.
			let file_path = Path::new(".btu_scheduler.toml");
			if ! file_path.exists() {
				return Err(ConfigError::MissingConfigFile);
			}

			let file_contents: String = fs::read_to_string(file_path)
				.expect("Something went wrong while reading the TOML file.");
			// println!("Here are the contents of the TOML configuration file: {}", file_contents);

			let result = AppConfig::new_from_toml_string(&file_contents);
			result
			// println!("{}", config);  // uses the Display trait defined below.
		}
	}

	impl fmt::Display for AppConfig {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			write!(f, "Application Configuration:\n
Seconds Between Refresh: {}\n
MySQL:\n
  Username: {}\n
  Password: {}\n
  Host: {}.{:?}\n
  Database: {}\n
RQ Host: {}\n
RQ Port: {}",
		  		self.max_seconds_between_updates,
				self.mysql_user,
				"********",
				self.mysql_host,
				self.mysql_port.unwrap_or(3306),
				self.mysql_database,
				self.rq_host,
				self.rq_port
			)
		}
	}

	pub fn get_mysql_conn(config: &AppConfig) -> Result<mysql::PooledConn, mysql::error::Error> {
		/* The purpose of this function is to:
			1. Create a formatted URL of MySQL connection arguments.
			2. Using that URL, create an activate MySQL connection object.
		*/
		let url = format!("mysql://{user}:{password}@{host}:{port}/{database}",
			user=config.mysql_user,
			password=config.mysql_password,
			host=config.mysql_host,
			port=config.mysql_port.unwrap_or(3306),  // default port for MySQL databases.
			database=config.mysql_database);

		let opts = mysql::Opts::from_url(&url)?;
		let pool = mysql::Pool::new(opts)?;
		pool.get_conn()
	}

	pub fn get_mysql_pool(config: &AppConfig) -> Result<mysql::Pool, mysql::error::Error> {
		/* The purpose of this function is to:
			1. Create a formatted URL of MySQL connection arguments.
			2. Using that URL, create an activate MySQL connection object.
		*/
		let url = format!("mysql://{user}:{password}@{host}:{port}/{database}",
			user=config.mysql_user,
			password=config.mysql_password,
			host=config.mysql_host,
			port=config.mysql_port.unwrap_or(3306),  // default port for MySQL databases.
			database=config.mysql_database);

		let opts = Opts::from_url(&url)?;
		Pool::new(opts)
	}	
}

pub mod task_scheduler {

	use chrono::{DateTime, Utc}; // See also: Local, TimeZone
	use cron::Schedule;
	use mysql::{PooledConn};
	use mysql::prelude::Queryable;
	use std::str::FromStr;
	use super::config;

	// Deliberately excluding SQL column that don't matter for this program.
	#[derive(Debug, Clone)]
	pub struct BtuTaskSchedule {
		id: String,
		task: String,
		task_description: String,
		enabled: String,
		queue_name: String,
		redis_job_id: String,
		argument_overrides: Option<String>,  // MUST use Option here, if the result is at all Nullable.
		schedule_description: String,
		cron_string: String,
	}

	pub fn read_btu_task_schedule(app_config: &config::AppConfig, task_schedule_id: &str) -> Option<BtuTaskSchedule> {
		/* Purpose: Given a Task Schedule identifier (string), connect to MySQL, query the table,
		            and return a new instance of struct 'BtuTaskSchedule' to the caller.
		*/

		// 1. Get a SQL connection.
		let mut sql_conn: PooledConn = config::get_mysql_conn(&app_config).unwrap();

		// 2. Run query, and map result into a new Result<Option<BtuTaskSchedule>>

		// TODO: Need to resolve SQL injection possibility.  Probably means crabbing some more Crates.
		let query_syntax = format!("SELECT name, task, task_description, enabled, queue_name,
		redis_job_id, argument_overrides, schedule_description, cron_string
		FROM `tabBTU Task Schedule` WHERE name = '{}'", task_schedule_id);

		/* TODO: exec_map appears entirely broken.
			thread '<unnamed>' panicked at 'Could not retrieve alloc::string::String from Value', 
			/home/sysop/.cargo/registry/src/github.com-1ecc6299db9ec823/mysql_common-0.27.5/src/value/convert/mod.rs:175:23
		*/

		let task_schedules: Vec<BtuTaskSchedule> = sql_conn
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
				}
			}).unwrap();
	
		if let Some(btu_task_schedule) =  task_schedules.iter().next() {
			Some(btu_task_schedule.to_owned())  // <--- function returns here
		} else {
			// Destructure failed. Change to the failure case.
			println!("Error: Was unable to read the SQL database and find a record for BTU Task Schedule = {}", task_schedule_id);
			None
		}       
	} // end of 'read_btu_task_schedule'

	// Entry point for building new Redis Queue Jobs.
	pub fn add_task_schedule_to_rq(app_config: &config::AppConfig, task_schedule: &BtuTaskSchedule) -> () {
		// Entry point for building new Redis Queue Jobs.

		// println!("{}", app_config);
		println!("Okay, I have all the task data for {}.  Now I need to create an RQ schedule from it.", task_schedule.id);

		/* Variables needed:
		cron_string, func, args=None, kwargs=None, repeat=None,
		queue_name=None, id=None, timeout=None, description=None, meta=None, use_local_timezone=False,
		depends_on=None, on_success=None, on_failure=None):
		*/

		let next_runtime = get_next_scheduled_time_utc(&task_schedule.cron_string);
		if next_runtime.is_none() {
			println!("Cannot determine Next Runtime for this Task.");
			()
		}
	
		println!("This BTU Task should run next at {:#?}", next_runtime.unwrap());
		println!("Establishing a connection to Redis RQ database on host {}, port {}", app_config.rq_host, app_config.rq_port);
		
		/*
		// Set result_ttl to -1, as jobs scheduled via cron are periodic ones.
   		// Otherwise the job would expire after 500 sec.

   		job = self._create_job(func, args=args, kwargs=kwargs, commit=False,
						  result_ttl=-1, id=id, queue_name=queue_name,
						  description=description, timeout=timeout, meta=meta, depends_on=depends_on,
						  on_success=on_success, on_failure=on_failure)

   		job.meta['cron_string'] = cron_string
   		job.meta['use_local_timezone'] = use_local_timezone
	   	if repeat is not None:
			job.meta['repeat'] = int(repeat)
	   	job.save()

   		self.connection.zadd(self.scheduled_jobs_key, {job.id: to_unix(scheduled_time)})
   		return job
		*/
	}


	/*
	
	REDIS_HOST
	REDIS_PORT
	REDIS_DB_NUMBER

	add_task_to_rq(
		cron_string,                # A cron string (e.g. "0 0 * * 0")
		func=func,                  # Python function to be queued
		args=[arg1, arg2],          # Arguments passed into function when executed
		kwargs={'foo': 'bar'},      # Keyword arguments passed into function when executed
		repeat=10,                  # Repeat this number of times (None means repeat forever)
		queue_name=queue_name,      # In which queue the job should be put in
		meta={'foo': 'bar'},        # Arbitrary pickleable data on the job itself
		use_local_timezone=False    # Interpret hours in the local timezone
	)

	read_task_from_sql()


	local_cron_to_utc_cron() {

	}

	list_jobs_in_rq() {

	}

	cancel_scheduled_task(task_schedule_identifier) {

	}
	*/

	fn cron_strn_to_cron_str7 (cron_expression_string: &str) -> Result<String, String> {
		/*
		Given a cron string of N elements, transform into a cron string of 7 elements.

		Reasoning: There is no universal standard for cron strings.  They could contain 5-7 elements.  However
		           the 'cron' library expects 7 elements.  This function pads the missing elements.
		*/

		let iter = cron_expression_string.split_whitespace();
		let vec: Vec<&str> = iter.collect::<Vec<&str>>();

		match vec.len() {
			5 =>  {
				// Prefix with '*' for seconds, and suffix with '*' for years.
				return Ok(format!("* {} *", cron_expression_string));
			},
			6 => {
				// Assume we're dealing with a cron(5) plus Year.  So prefix '*' for seconds.
				return Ok(format!("* {}", cron_expression_string));
			},	
			7 => {
				// Cron string already has 7 elements, so pass it back.
				return Ok(cron_expression_string.to_owned())
			},
			_ => {
				return Err(format!("Cron expression string has {} values", vec.len()));
			}				
		}
	}

	fn get_next_scheduled_time_utc(cron_expression_string: &str) -> Option<DateTime<Utc>> {
    	/*
			Based on a cron string, what is the next, scheduled Datetime?
			Documentation: https://docs.rs/cron/0.9.0/cron

			Note: The 'cron' library expects a 7-element cron string.  Where the additional elements are:
				--> Seconds
					Minutes
					Hours
					Day of Month
					Month
					Day of Week
				--> Years
		*/

		match cron_strn_to_cron_str7(cron_expression_string) {
			Ok(cron_string) => {

				// We now have a 7-element cron string.
				match Schedule::from_str(&cron_string) {
					Ok(schedule) => {
						return schedule.upcoming(Utc).take(10).next();
					},
					Err(error) => {
						println!("ERROR: Cannot parse invalid cron string: '{}'.  Error: {}", cron_string, error);
						return None;
					}
				}
			},
			Err(error) => {
				println!("ERROR: Cannot parse invalid cron string: '{}'.  Error: {}", cron_expression_string, error);
				return None;
			}
		}
	} // end function 'get_next_scheduled_time_utc'
}