#![forbid(unsafe_code)]
#![allow(dead_code)]

// Main library 'btu_scheduler'
// These modules are located in adjacent files.
use mysql::PooledConn;
use mysql::prelude::Queryable;
use serde::Deserialize;

pub mod btu_cron;
pub mod config;
pub mod rq;
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
	
	#[derive(ThisError, Debug, PartialEq)]
	pub enum CronError {
		#[error("Cron expression has the wrong number of elements (should be one of 5, 6, or 7).")]
		WrongQtyOfElements {
			found: usize
		},
		#[error("Invalid cron expression; could not transform into a CronStruct.")]
		InvalidExpression
	}

	#[derive(ThisError, Debug, PartialEq)]
	pub enum StringError {
		#[error("Element cannot be split using delimiter.")]
		MissingDelimiter,
	}

	#[derive(ThisError, Debug, PartialEq)]
	pub enum RQError {
		#[error("No idea what happened here.")]
		Unknown {
			#[from]
			source: redis::RedisError,
		}
	}
}  // end of error module.

pub mod task {
	
	use std::fmt;
	use mysql::prelude::Queryable;
	use mysql::PooledConn;
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
			println!("mysql Row named foo = {:?}", row);

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
			println!("{}", task);
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

	pub fn print_enabled_tasks(app_config: &AppConfig) -> () {

		let mut sql_conn: PooledConn;
		match config::get_mysql_conn(app_config) {
			Ok(_conn) => {
				sql_conn = _conn;
			},
			Err(err) => {
				println!("Error while attempting to get connection in 'query_task_summary' : {}", err);
				return ()
			}
		}

		let query_syntax = "SELECT name, desc_short	FROM `tabBTU Task` WHERE docstatus = 1";
		let task_vector: Vec<(String,String)> = sql_conn.query_map(query_syntax, |row: mysql::Row| {
			(row.get(0).unwrap(), row.get(1).unwrap())
		}).unwrap();

		if task_vector.len() > 0 {
			for task in task_vector {
				println!("Task {} : {}", task.0, task.1);
			}
		}
		else {
			println!("No BTU Tasks are defined in the MariaDB database.");
		}
	}
}  // end of task module.

pub mod task_schedule {
	
	use std::convert::TryFrom;
	use chrono_tz::Tz;
	use mysql::PooledConn;
	use mysql::prelude::Queryable;
	use crate::config::{self, AppConfig};
	use crate::rq::RQJob;
	use crate::task::BtuTask;

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
		enabled: u8,
		queue_name: String,
		redis_job_id: String,
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
		pub fn to_rq_job(&self, app_config: &AppConfig) -> RQJob {

			let mut new_job: RQJob = RQJob::new_with_defaults();
			new_job.description = self.task_description.clone();
			match crate::get_pickled_function_from_web(&self.task, Some(&self.id), app_config) {
				Ok(byte_result) => {
					new_job.data = byte_result;
				}
				Err(error_message) => {
					panic!("Error while requesting pickled Python function:\n{}", error_message);
				}
			}
			new_job
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
					cron_timezone: row.get::<String, _>(9).unwrap().parse().unwrap()
				}
			}).unwrap();
	
		// There is either exactly 1 SQL row, or zero.
		// Therefore the syntax below uses 'next()' to either fetch it, or return a None.
		if let Some(btu_task_schedule) =  task_schedules.iter().next() {
			Some(btu_task_schedule.to_owned())
		} else {
			None  // no such record in the MySQL table.
		}       
	}
}

pub mod scheduler {

	use std::collections::VecDeque;
	use std::fmt;
	use chrono::{DateTime, Utc}; // See also: DateTime, Local, TimeZone
	use chrono::NaiveDateTime;
	use redis::{self, Commands, RedisError};
	use crate::{btu_cron, config, rq};
	use crate::task_schedule::{BtuTaskSchedule, read_btu_task_schedule};

	// static RQ_SCHEDULER_NAMESPACE_PREFIX: &'static str = "rq:scheduler_instance:";
	// static RQ_KEY_SCHEDULER: &'static str = "rq:scheduler";
    // static RQ_KEY_SCHEDULER_LOCK: &'static str = "rq:scheduler_lock";
    static RQ_KEY_SCHEDULED_TASKS: &'static str = "btu_scheduler:task_execution_times";

	#[derive(Debug, PartialEq, Clone)]
	pub struct RQScheduledTask {
		pub task_schedule_id: String,
		pub next_datetime_unix: i64,
		pub next_datetime_utc: DateTime<Utc>,
	}

	impl std::fmt::Display for RQScheduledTask {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		  write!(f, "{} at {}",
		      self.task_schedule_id,
			  self.next_datetime_utc)
		}
	}

	impl From<(String,String)> for RQScheduledTask {

		// TODO: Change to a Result with some kind of Error
		fn from(tuple: (String, String)) -> RQScheduledTask {
			_from_tuple_to_rqscheduledtask(&tuple).unwrap()
		}
	}

	fn _from_tuple_to_rqscheduledtask(tuple: &(String, String)) -> Result<RQScheduledTask, std::io::Error> {
		/* 
			The tuple argument consists of 2 Strings: JobId and Unix Timestamp.
			Using this information, we can build an RQScheduledTask struct.
			There is no reason to consume the tuple; so accepting it as a reference.
		*/
		let timestamp: i64 = tuple.1.parse::<i64>().unwrap();  // coerce the second String into an i64, using "turbofish" syntax
		let utc_datetime:  DateTime<Utc> = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc);
		/*
		let local_time_zone: chrono_tz::Tz = match tuple.2.parse() {
			Ok(v) => v,
			Err(e) => {
				let new_error = std::io::Error::new(std::io::ErrorKind::Other, "Could not match local time zone to chrono Tz.");
				return Err(new_error)
			}
		};
		 */
		Ok(RQScheduledTask {
			task_schedule_id: tuple.0.clone(),
			next_datetime_unix: timestamp,
			next_datetime_utc: utc_datetime
			// next_datetime_local: local_time_zone.from_utc_datetime(&utc_datetime.naive_utc())
		})
	}

	// NOTE:  Below is the 'Newtype Pattern'.
	// It's useful when you need implement Traits you aren't normally allowed to: because you don't own either
	// the Trait or Type.  In this case, I don't the "From" or "FromIterator" traits, nor the "Vector" type.
	// But I wrap Vec<RQScheduledTask> in a Newtype, and I can do whatever I want with it.

	pub struct VecRQScheduledTask ( Vec<RQScheduledTask> );

	impl VecRQScheduledTask {

		fn new() -> Self {
			let empty_vector: Vec<RQScheduledTask> = Vec::new();
			VecRQScheduledTask(empty_vector)
		}

		fn len(&self) -> usize {
			// Because this is just a 1-element tuple, "self.0" gets the inner Vector!
			self.0.len()
		}

		fn sort_by_id(self) -> VecRQScheduledTask {
			// Consumes the current VecRQScheduledTask, and returns another that is sorted by Task Schedule ID.
			let mut result = self.0;
			result.sort_by(|a, b| a.task_schedule_id.partial_cmp(&b.task_schedule_id).unwrap());
			VecRQScheduledTask(result)
		}
		
		fn sort_by_next_datetime(self) -> VecRQScheduledTask {
			// Consumes the current VecRQScheduledTask, and returns another that is sorted by Task Schedule ID.
			let mut result = self.0;
			result.sort_by(|a, b| a.next_datetime_unix.partial_cmp(&b.next_datetime_unix).unwrap());
			VecRQScheduledTask(result)
		}

	}

	impl std::iter::FromIterator<Result<RQScheduledTask, std::io::Error>> for VecRQScheduledTask {

		fn from_iter<T>(iter: T) -> VecRQScheduledTask
		where T: IntoIterator<Item=Result<RQScheduledTask, std::io::Error>> {
			
			let mut result: VecRQScheduledTask = VecRQScheduledTask::new();
			for inner in iter {
				result.0.push(inner.unwrap()); 
			}
			result
		}
	}

	// Create a 3rd struct which will contain a reference to your set of data.
	struct IterNewType<'a> {
		inner: &'a VecRQScheduledTask,
		// position used to know where you are in your iteration.
		pos: usize,
	}
	
	// Now you can just implement the `Iterator` trait on your `IterNewType` struct.
	impl<'a> Iterator for IterNewType<'a> {

		type Item = &'a RQScheduledTask;

		fn next(&mut self) -> Option<Self::Item> {
			if self.pos >= self.inner.len() {
				// No more data to read, so stop here.
				None
			} else {
				// We increment the position of our iterator.
				self.pos += 1;
				// We return the current value pointed by our iterator.
				self.inner.0.get(self.pos - 1)
			}
		}
	}
	
	impl VecRQScheduledTask {
		fn iter<'a>(&'a self) -> IterNewType<'a> {
			IterNewType {
				inner: self,
				pos: 0,
			}
		}
	}

	impl From<Vec<(String,String)>> for VecRQScheduledTask {

		fn from(vec_of_tuple: Vec<(String,String)>) -> Self {

			if vec_of_tuple.len() == 0 {
				// If passed an empty tuple, return an empty Vector of RQScheduledTask.
				return VecRQScheduledTask::new();
			}
			let result = vec_of_tuple.iter().map(_from_tuple_to_rqscheduledtask).collect();
			result
		}
	}

	/**
	 	This function writes a Task Schedules "Next Execution Time(s)" to the Redis Queue database.
	*/ 
	pub fn add_task_schedule_to_rq(app_config: &config::AppConfig, task_schedule: &BtuTaskSchedule) -> () {
		/*
			Developer Notes:
			
			1. The only caller for this function is Thread #1.

			2. This function's concept was derived from the Python 'rq_scheduler' library.  In that library, the public
			   entrypoint (from the website) was named a function 'cron()'.  That function did a few things:

			   * Created an RQ Job object in the Redis datbase.
			   * Calculated that Job's next execution time, in UTC.
  			   * Added a Z key to Redis where the "Score" is the Next UTC Runtime, expressed as a Unix Time.
				 self.connection.zadd("rq:scheduler:scheduled_jobs", {job.id: to_unix(scheduled_time)})

			3. I am making a deliberate decision to -not- create an RQ Job at this time.  But instead, to create the RQ
			   Job later, when it's time to actually run it.
			   
			   My reasoning is this: a Frappe web user might edit the definition of a Task between the time it was scheduled
			   in RQ, and the time it actually executes.  This would make the RQ Job stale and invalid.  So anytime someone edits
			   a BTU Task, I would have to rebuild all related Task Schedules.  Instead, by waiting until execution time, I only have
			   to react to Schedule modfications in the Frappe web app; not Task modifications.

			   The disadvantage is that if the Frappe Web Server is not online and accepting REST API requests, when it's
			   time to run a Task Schedule?  Then BTU Scheduler will fail.  Because it needs Frappe alive to construct a pickled
			   RQ Job.

			   Of course, if the Frappe web server is offline, there's a good chance the BTU Task Schedule might fail anyway.
			   So I think the benefits of waiting to create RQ Jobs outweights the drawbacks.
		*/
		println!("Calculating next execution times for BTU Task Schedule {} : ", task_schedule.id);
		let next_runtimes: Vec<DateTime<Utc>> = btu_cron::tz_cron_to_utc_datetimes(&task_schedule.cron_string,
			                                                                       task_schedule.cron_timezone,
																				   None,
																				   1).unwrap();
		if next_runtimes.len() == 0 {
			println!("ERROR: Cannot calculate the any 'Next Execution Time' values for Task Schedule {}", &task_schedule.id);
			return ()
		}

		/*
		  Developer Note: I am only retrieving the 1st value from the result vector.
		                  Later, it might be helpful to use multiple Next Execution Times,
						  because of time zone shifts around Daylight Savings.
		*/

		// If application configuration has a good Time Zone string, print Next Execution Time in local time...
		if let Ok(timezone) = app_config.tz() {
			println!("* Next Execution Time: {}", next_runtimes[0].with_timezone(&timezone).to_rfc2822());	
		}
		// Always print in UTC.
		println!("* Next Execution Time: {} UTC", next_runtimes[0].to_rfc3339());

		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");

		// The Redis 'zadd()' operation returns an integer.
		let some_result: Result<std::primitive::u32, RedisError>;
		some_result = redis_conn.zadd(RQ_KEY_SCHEDULED_TASKS, &task_schedule.id, next_runtimes[0].timestamp());

		match some_result {
			Ok(_result) => {
				// Developer Note: I believe a result of 1 means Redis wrote a new record.
				//                 A result of 0 means the record already existed, and no write was necessary.
				// println!("Result from 'zadd' is Ok, with the following payload: {}", result);
			},
			Err(error) => {
				println!("Result from 'zadd' is Err, with the following payload: {}", error);
			}
		}
		/*
			Developer Notes:
			* If you were to examine Redis at this time, the "Score" is the Next Execution Time (as a Unix timestamp),
			and the "Member" is the BTU Task Schedule identifier.
			* We haven't created an RQ Jobs for this Task Schedule yet.
		*/
   		()
	}

	fn fetch_task_schedules_ready_for_rq(app_config: &config::AppConfig, sched_before_unix_time: i64) -> Vec<String> {
		// Read the BTU section of RQ, and return the Jobs that are scheduled to execute before a specific Unix Timestamp.

		// Developer Notes: Some cleverness below, courtesy of 'rq-scheduler' project.  For this particular key,
		// the Z-score represents the Unix Timestamp the Job is supposed to execute on.
		// By fetching ALL the values below a certain threshhold (Timestamp), the program knows precisely which Jobs
		// to enqueue...

		println!("Upcoming Task Schedules:");
		rq_print_scheduled_tasks(&app_config);

		println!("Reviewing the 'Next Execution Times' for each Task Schedule in Redis...");
		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish connection with Python RQ database server.");
		
		// TODO: As per Redis 6.2.0, the command 'zrangebyscore' is considered deprecated.
		// Please prefer using the ZRANGE command with the BYSCORE argument in new code.
		let redis_result: Result<Vec<String>, redis::RedisError> = redis_conn.zrangebyscore(RQ_KEY_SCHEDULED_TASKS, 0, sched_before_unix_time);
		if redis_result.is_ok() {
			let jobs_to_enqueue = redis_result.unwrap();
			if jobs_to_enqueue.len() > 0 {
				println!("Found {:?} Task Schedules that qualify for immediate execution.", jobs_to_enqueue);
			}
			jobs_to_enqueue  // return a Vector of Task Schedule identifiers
		}
		else {
			Vec::new()
		}
	}

	/**
	   Examine the Next Execution Time for all scheduled RQ Jobs (this information is stored in RQ as a Unix timestamps)
       If the Next Execution Time is in the past?  Then place the RQ Job into the appropriate queue.  RQ and Workers take over from there.
	*/

	pub fn check_and_run_eligible_task_schedules(app_config: &config::AppConfig, internal_queue: &mut VecDeque<String>) {
		// Developer Note: This function is analgous to the 'rq-scheduler' Python function: 'Scheduler.enqueue_jobs()'
		let unix_timestamp_now = Utc::now().timestamp();
        let task_schedule_keys = fetch_task_schedules_ready_for_rq(app_config, unix_timestamp_now);
		for task_schedule_key in task_schedule_keys.iter() {
			println!("Time to make the donuts! (enqueuing Redis Job '{}' for immediate execution)", task_schedule_key);
			match run_immediate_scheduled_task(app_config, &task_schedule_key, internal_queue) {
				Ok(_) => {
				},
				Err(err) => {
					println!("Error while attempting to run Task Schedule {} : {}", task_schedule_key, err);
				}
			}
		}
	}

	pub fn run_immediate_scheduled_task(app_config: &config::AppConfig, 
		                                task_schedule_id: &str,
										internal_queue: &mut VecDeque<String>) -> Result<String,String> {

		// 0. Remove the Task from the Schedule (so it doesn't get executed twice)
		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
		let redis_result: redis::RedisResult<u64> = redis_conn.zrem(RQ_KEY_SCHEDULED_TASKS, task_schedule_id);
		if redis_result.is_err() {
			return Err(redis_result.err().unwrap().to_string());
		}

		// 1. Read the MariaDB database to construct a BTU Task Schedule struct.
		let task_schedule = read_btu_task_schedule(app_config, task_schedule_id);
		if task_schedule.is_none() {
			return Err("Unable to read Task Schedule from MariaDB database.".to_string());
		}
		let task_schedule: BtuTaskSchedule = task_schedule.unwrap();  // shadow original variable.

		// 3. Create an RQ Job from the BtuTask struct.
		let rq_job: rq::RQJob = task_schedule.to_rq_job(app_config);
		println!("Created an RQJob struct: {}", rq_job);

		// 4. Save the new Job into Redis.
		rq_job.save_to_redis(app_config);

		// 5. Enqueue that job for immediate execution.
		match rq::enqueue_job_immediate(&app_config, &rq_job.job_key_short) {
			Ok(ok_message) => {
				println!("Successfully enqueued: {}", ok_message);
			}
			Err(err_message) => {
				println!("Error while attempting to queue job for execution: {}", err_message);
			}
		}
		/* 6. Recalculate the next Run Time.
			  Which is very easy: just push the Task Schedule ID back into the Internal Queue! :)
		*/
		internal_queue.push_back(task_schedule_id.to_string());
		Ok("".to_string())
	}

	pub fn rq_get_scheduled_tasks(app_config: &config::AppConfig) -> VecRQScheduledTask {
		/*
			Call RQ and request the list of values in "btu_scheduler:job_execution_times"
		*/
		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
		let redis_result: Vec<(String, String)> = redis_conn.zscan(RQ_KEY_SCHEDULED_TASKS).unwrap().collect();
		let wrapped_result: VecRQScheduledTask = redis_result.into();
		wrapped_result		
	}
	
	/**
		Remove a Task Schedule from the Redis database, to prevent it from executing in the future.
	*/	
	pub fn rq_cancel_scheduled_task(app_config: &config::AppConfig, task_schedule_id: &str) -> Result<String,String> {
		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
		let redis_result: redis::RedisResult<u64> = redis_conn.zrem(RQ_KEY_SCHEDULED_TASKS, task_schedule_id);
		if redis_result.is_err() {
			return Err(redis_result.err().unwrap().to_string());
		}
		return Ok("Scheduled Task successfully removed from Redis Queue.".to_owned());
	}

	/**
		Prints upcoming Task Schedules using the configured Time Zone.
	*/
	pub fn rq_print_scheduled_tasks(app_config: &config::AppConfig) {

		let tasks: VecRQScheduledTask = rq_get_scheduled_tasks(app_config);  // fetch all the scheduled tasks.
		let local_time_zone: chrono_tz::Tz = app_config.tz().unwrap();  // get the time zone from the Application Configuration.

		for result in tasks.sort_by_id().iter() {
			let next_datetime_local = result.next_datetime_utc.with_timezone(&local_time_zone);
			println!("{} at {}", result.task_schedule_id, next_datetime_local);
		};
	}

	/*
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
	*/
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
	// println!("Response as JSON: {:?}", response_json.message);
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