#![forbid(unsafe_code)]
pub mod config {

	use std::fmt;
	use std::fs;
	use std::path::Path;
	use serde::{Deserialize};  // Also there is Serialize
	use mysql::*;


	#[derive(Deserialize)]
	pub struct AppConfig {
		pub max_seconds_between_updates: u32,
		mysql_user: String,
		mysql_password: String,
		mysql_host: String,
		mysql_port: Option<u32>,
		mysql_database: String
	}

	impl AppConfig {
    	// Associated function signature; `Self` refers to the implementor type.
		pub fn new_from_toml_file() -> AppConfig {

			// Read TOML file, and store values here in this configuration.
			let file_path = Path::new(".py_sched.toml");
			if ! file_path.exists() {
				panic!("Cannot find expected file on disk: '.py_sched.toml'");
			}

			let file_contents: String = fs::read_to_string(file_path)
				.expect("Something went wrong reading the file");

			// println!("Here are the contents of the TOML configuration file: {}", file_contents);

			// TODO: Replace with some friendlier error handling, instead of a panic.
			let config: AppConfig = toml::from_str(&file_contents).unwrap();
			// println!("{}", config);  // uses the Display trait defined below.
			config
		}
	}

	impl fmt::Display for AppConfig {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			write!(f, "Application Configuration:\nSeconds Between Refresh: {}\nMySQL:\n  Username: {}\n  Password: {}\n  Host: {}.{:?}\n  Database: {}\n",
		  		self.max_seconds_between_updates,
				self.mysql_user,
				"********",
				self.mysql_host,
				self.mysql_port.unwrap_or(3306),
				self.mysql_database
			)
		}
	}

	pub fn get_mysql_conn(config: &AppConfig) -> Result<mysql::PooledConn> {
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
		let pool = Pool::new(opts)?;
		pool.get_conn()
	}

	pub fn get_mysql_pool(config: &AppConfig) -> Result<mysql::Pool> {
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

use super::config;
	use mysql::{PooledConn};
	use mysql::prelude::Queryable;

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
			Some(btu_task_schedule.to_owned())
		} else {
			// Destructure failed. Change to the failure case.
			println!("Error: Was unable to read the SQL database and find a record for BTU Task Schedule = {}", task_schedule_id);
			None
		}       
	} // end function

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

		// scheduled_time = get_next_scheduled_time(cron_string, use_local_timezone=use_local_timezone)


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


	// sudo systemctl start rqscheduler.service
	// sudo systemctl status rqscheduler.service
	// sudo systemctl enable rqscheduler.service

}