#![forbid(unsafe_code)]
#![allow(dead_code)]

// Main library 'btu_scheduler'
// These modules are located in adjacent files.
pub mod btu_cron;
pub mod config;
pub mod rq;
mod tests;

pub fn get_package_version() -> &'static str {
    // Completed.
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    VERSION
}

pub mod task {
	
	use std::fmt;
	use mysql::prelude::Queryable;
	use mysql::PooledConn;
	use std::io::Read;  // mandatory for 'resp.into_reader()' call in get_pickled_function_from_web
	use crate::config;
	use crate::config::AppConfig;
	use crate::rq::RQJob;

	pub struct BtuTask {
		pub task_key: String,
		desc_short: String,
		desc_long: String,
		arguments: String,
		path_to_function: String,	// example:  btu.manual_tests.ping_with_wait
		max_task_duration: String,  // example:  600s
	}

	// TODO: Need to resolve SQL injection possibility.  Probably means crabbing some more Crates.

	impl BtuTask {

		/// Call ERPNext REST API and acquire pickled Python function as bytes.
		fn get_pickled_function_from_web(&self, app_config: &AppConfig) -> Result<Vec<u8>, String> {

			let url: String = format!("http://{}:{}/api/method/btu.btu_api.endpoints.get_pickled_task",
				app_config.webserver_ip, app_config.webserver_port);
		
			let wrapped_response = ureq::get(&url)
				.set("Authorization", &app_config.webserver_token)
				.set("Content-Type", "application/json")
				//.set("Content-Type", "application/octet-stream")
				.send_json(ureq::json!({
					"task_id": self.task_key
					})
				);
			if wrapped_response.is_err() {
				return Err(format!("Error in response: {:?}", wrapped_response.err()));
			}

			let resp = wrapped_response.unwrap();
			assert!(resp.has("Content-Length"));
			let len = resp.header("Content-Length")
				.and_then(|s| s.parse::<usize>().ok()).unwrap();
		
			let mut bytes: Vec<u8> = Vec::with_capacity(len);
			// Read the bytes, up to a maximum:
			resp.into_reader()
				.take(10_000_000)
				.read_to_end(&mut bytes).unwrap();
		
			if bytes.len() != len {
				return Err(format!("Expected {} bytes, but only {} bytes were retrieved.", len, bytes.len()))
			}
			println!("HTTP Response from 'get_pickled_task': {} total bytes.", bytes.len());
			Ok(bytes)
		}

		pub fn new_from_mysql(task_key: &str, app_config: &AppConfig) -> Self {
			let mut sql_conn: PooledConn = config::get_mysql_conn(app_config).unwrap();

			let query_syntax = format!("SELECT name AS task_key, desc_short, desc_long,
			arguments, function_string AS path_to_function,	max_task_duration 
			FROM `tabBTU Task` WHERE name = '{}'", task_key);

			// Brian: the 2 lines below work too, if you just want a simple MySQL Row.
			// let foo: mysql::Row = sql_conn.query_first(&query_syntax).unwrap().unwrap();
			// println!("mysql Row named foo = {:?}", foo);

			let task: BtuTask = sql_conn.query_first(query_syntax).unwrap().map(|row: mysql::Row| {
					BtuTask {
						task_key: row.get(0).unwrap(),
						desc_short: row.get(1).unwrap(),
						desc_long: row.get(2).unwrap(),
						arguments:  row.get(3).unwrap(),
						path_to_function:  row.get(4).unwrap(),
						max_task_duration:  row.get(5).unwrap()
					}
				}).unwrap();
			task
		}

		/// Create an RQ Job struct from a BTU Task struct.
		pub fn to_rq_job(&self, app_config: &AppConfig) -> RQJob {

			let mut new_job: RQJob = RQJob::new_with_defaults();
			new_job.description = self.desc_long.clone();
			match self.get_pickled_function_from_web(app_config) {
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

	impl fmt::Display for BtuTask {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			// This syntax helpfully ignores the leading whitespace on successive lines.
			write!(f,  "task_key: {}\n\
						desc_short: {}\n\
						desc_long: {}\n\
						arguments: {}\n\
						path_to_function: {}\n\
						max_task_duration: {}",
				self.task_key, self.desc_short,
				self.desc_long, self.arguments, self.path_to_function, self.max_task_duration
			)
		}
	}

	pub fn query_task_summary(app_config: &AppConfig) -> Option<Vec<(String, String)>> {

		let mut sql_conn: PooledConn;
		match config::get_mysql_conn(app_config) {
			Ok(_conn) => {
				sql_conn = _conn;
			},
			Err(err) => {
				println!("Error while attempting to get connection in 'query_task_summary' : {}", err);
				return None
			}
		}
		
		let query_syntax = "SELECT name, desc_short	FROM `tabBTU Task` WHERE docstatus = 1";

		let task_vector: Vec<(String,String)> = sql_conn.query_map(query_syntax, |row: mysql::Row| {
			(row.get(0).unwrap(), row.get(1).unwrap())
		}).unwrap();

		if task_vector.len() > 0 {
			return Some(task_vector)
		}
		return None
	}
}  // end of task module.

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
}

pub mod task_scheduler {

	use chrono::{DateTime, Utc}; // See also: DateTime, Local, TimeZone
	use chrono::NaiveDateTime;
	use mysql::PooledConn;
	use mysql::prelude::Queryable;
	use redis::{self};
	use redis::{Commands, RedisError};

	use crate::btu_cron;
	use crate::config;
	use crate::rq;
	
	static RQ_SCHEDULER_NAMESPACE_PREFIX: &'static str = "rq:scheduler_instance:";
	static RQ_KEY_SCHEDULER: &'static str = "rq:scheduler";
    static RQ_KEY_SCHEDULER_LOCK: &'static str = "rq:scheduler_lock";
    static RQ_KEY_SCHEDULED_JOBS: &'static str = "btu_scheduler:job_execution_times";

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
		cron_timezone: chrono_tz::Tz
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

		// TODO: Stop hardcoding this, and fetch from SQL Table `tabBTU Task Schedule`
		let eastern_timezone = chrono_tz::America::New_York;

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
					cron_timezone: eastern_timezone
				}
			}).unwrap();
	
		// Get the first BtuTaskSchedule struct in the vector.
		// Because 'name' is the MySQL table's Primary Key, there is either 0 or 1 values.
		if let Some(btu_task_schedule) =  task_schedules.iter().next() {
			Some(btu_task_schedule.to_owned())
		} else {
			None  // no such record in the MySQL table.
		}       
	}

	// Entry point for building new Redis Queue Jobs.
	pub fn add_task_schedule_to_rq(app_config: &config::AppConfig, task_schedule: &BtuTaskSchedule) -> () {
		/*
			Summary: Main entry point for scheduling new Redis Queue Jobs.

			In the Python 'rq_scheduler' library, the public entrypoint (from the website) was named a function 'cron()'.
			That function does a few things, which we'll need to replicate here:
  
 			1. Get the next scheduled execution time, in UTC.
			2. Create an RQ 'job' Object
			3. Adds a Z key to Redis where the "Score" is the Next UTC Runtime, expressed as a Unix Time.
				self.connection.zadd("rq:scheduler:scheduled_jobs", {job.id: to_unix(scheduled_time)})
		*/

		println!("Scheduling BTU Task Schedule: '{}'", task_schedule.id);
		let next_runtimes: Vec<DateTime<Utc>> = btu_cron::local_cron_to_utc_datetimes(&task_schedule.cron_string, task_schedule.cron_timezone, 10).unwrap();
		if next_runtimes.len() == 0 {
			println!("ERROR: Cannot calculate the next Next Runtime for Task {}", &task_schedule.id);
			()
		}
		println!("  * Next execution time for this Task is {}", next_runtimes[0]);

		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");

		/* Variables needed:
		cron_string, func, args=None, kwargs=None, repeat=None,
		queue_name=None, id=None, timeout=None, description=None, meta=None, use_local_timezone=False,
		depends_on=None, on_success=None, on_failure=None):
		*/

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

		*/

		// The Redis 'zadd()' operation returns an integer.
		let some_result: Result<std::primitive::u32, RedisError>;
		some_result = redis_conn.zadd(RQ_KEY_SCHEDULED_JOBS, &task_schedule.redis_job_id, next_runtimes[0].timestamp());

		match some_result {
			Ok(result) => {
				println!("Result from 'zadd' is Ok, with the following payload: {}", result);
			},
			Err(error) => {
				println!("Result from 'zadd' is Err, with the following payload: {}", error);
			}
		}
   		()
	}

	fn fetch_jobs_ready_for_rq(app_config: &config::AppConfig, sched_before_unix_time: i64) -> Vec<String> {
		// Read the BTU section of RQ, and return the Jobs that are scheduled to execute before a specific Unix Timestamp.

		// Developer Notes: Some cleverness below, courtesy of 'rq-scheduler' project.  For this particular key,
		// the Z-score represents the Unix Timestamp the Job is supposed to execute on.
		// By fetching ALL the values below a certain threshhold (Timestamp), the program knows precisely which Jobs
		// to enqueue...
		
		println!("Searching Redis for Jobs that can be immediately enqueued in RQ ...");
		println!("FYI, here are all the tasks:");
		rq_print_scheduled_tasks(&app_config);

		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish connection with RQ database server.");
		
		// TODO: As per Redis 6.2.0, the command 'zrangebyscore' is considered deprecated.
		// Please prefer using the ZRANGE command with the BYSCORE argument in new code.
		let redis_result: Result<Vec<String>, redis::RedisError> = redis_conn.zrangebyscore("rq:scheduler:scheduled_jobs", 0, sched_before_unix_time);
		if redis_result.is_ok() {
			let jobs_to_enqueue = redis_result.unwrap();
			if jobs_to_enqueue.len() > 0 {
				println!("Found {:?} jobs that qualify for immediate execution.", jobs_to_enqueue);
			}
			jobs_to_enqueue  // return a Vector of Job identifiers
		}
		else {
			Vec::new()
		}
	}

	pub fn promote_jobs_to_rq_if_ready(app_config: &config::AppConfig) {
		// This function is analgous to the 'rq-scheduler' Python function: 'Scheduler.enqueue_jobs()'

		let unix_timestamp_now = Utc::now().timestamp();
		// println!("Current Unix Timestamp is '{}'", unix_timestamp_now);

        let jobs_to_enqueue = fetch_jobs_ready_for_rq(app_config, unix_timestamp_now);
		for job in jobs_to_enqueue.iter() {
			println!("Time to make the donuts! (enqueuing Redis Job '{}' for immediate execution)", job);
			// self.enqueue_job(job)
		}
	}


	#[derive(Debug, PartialEq, Clone)]
	pub struct RQScheduledJob {
		pub job_id: String,
		pub start_datetime_unix: i64,
		pub start_datetime_utc: DateTime<Utc>
	}

	fn _from_tuple_to_rqscheduledjob(tuple: &(String, String)) -> Result<RQScheduledJob, std::io::Error> {
		/* 
			Use the tuple with 2 Strings to construct an RQScheduledJob struct.
			Cannot think of a reason why I should move/consume the tuple?  So I won't.
		*/
		let timestamp: i64 = tuple.1.parse::<i64>().unwrap();  // coerce the second String into an i64, using "turbofish" syntax
		Ok(RQScheduledJob {
			job_id: tuple.0.clone(),
			start_datetime_unix: timestamp,
			start_datetime_utc: DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc)
		})
	}

	impl From<(String,String)> for RQScheduledJob {

		// TODO: Change to a Result with some kind of Error
		fn from(tuple: (String, String)) -> RQScheduledJob {
			_from_tuple_to_rqscheduledjob(&tuple).unwrap()
		}
	}

	// NOTE:  Below is the 'Newtype Pattern'.
	// It's useful when you need implement Traits you aren't normally allowed to: because you don't own either
	// the Trait or Type.  In this case, I don't the "From" or "FromIterator" traits, nor the "Vector" type.
	// But I wrap Vec<RQScheduledJob> in a Newtype, and I can do whatever I want with it.

	pub struct VecRQScheduledJob ( Vec<RQScheduledJob> );
	impl VecRQScheduledJob {

		fn new() -> Self {
			let empty_vector: Vec<RQScheduledJob> = Vec::new();
			VecRQScheduledJob(empty_vector)
		}

		fn len(&self) -> usize {
			// Because this is just a 1-element tuple, "self.0" gets the inner Vector!
			self.0.len()
		}
	}

	/* OLD IMPLEMENTATION, before I returned a Result.

	impl std::iter::FromIterator<RQScheduledJob> for VecRQScheduledJob {

		fn from_iter<T>(iter: T) -> VecRQScheduledJob
		where T: IntoIterator<Item=RQScheduledJob> {
			
			let mut result: VecRQScheduledJob = VecRQScheduledJob::new();
			for inner in iter { 
				result.0.push(inner); 
			}
			result
		}
	}
	*/

	impl std::iter::FromIterator<Result<RQScheduledJob, std::io::Error>> for VecRQScheduledJob {

		fn from_iter<T>(iter: T) -> VecRQScheduledJob
		where T: IntoIterator<Item=Result<RQScheduledJob, std::io::Error>> {
			
			let mut result: VecRQScheduledJob = VecRQScheduledJob::new();
			for inner in iter {
				result.0.push(inner.unwrap()); 
			}
			result
		}
	}

	// Create a 3rd struct which will contain a reference to your set of data.
	struct IterNewType<'a> {
		inner: &'a VecRQScheduledJob,
		// position used to know where you are in your iteration.
		pos: usize,
	}
	
	// Now you can just implement the `Iterator` trait on your `IterNewType` struct.
	impl<'a> Iterator for IterNewType<'a> {

		type Item = &'a RQScheduledJob;

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
	
	impl VecRQScheduledJob {
		fn iter<'a>(&'a self) -> IterNewType<'a> {
			IterNewType {
				inner: self,
				pos: 0,
			}
		}
	}

	impl From<Vec<(String,String)>> for VecRQScheduledJob {

		fn from(vec_of_tuple: Vec<(String,String)>) -> Self {

			if vec_of_tuple.len() == 0 {
				// If passed an empty tuple, return an empty Vector of RQScheduledJob.
				return VecRQScheduledJob::new();
			}
			let result = vec_of_tuple.iter().map(_from_tuple_to_rqscheduledjob).collect();
			result
		}
	}

	pub fn rq_get_scheduled_tasks(app_config: &config::AppConfig) -> VecRQScheduledJob {
		/*
			Call RQ and request the list of values in "btu_scheduler:job_execution_times"
		*/
		let mut redis_conn = rq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
		let redis_result: Vec<(String, String)> = redis_conn.zscan(RQ_KEY_SCHEDULED_JOBS).unwrap().collect();
		let wrapped_result: VecRQScheduledJob = redis_result.into();
		wrapped_result		
	}
	
	pub fn rq_print_scheduled_tasks(app_config: &config::AppConfig) {
		let tasks: VecRQScheduledJob = rq_get_scheduled_tasks(app_config);
		for result in tasks.iter() {
			println!("{:#?}", result);
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

	local_cron_to_utc_cron() {

	}

	rq_print_scheduled_tasks() {

	}

	cancel_scheduled_task(task_schedule_identifier) {

	}
	*/
}
