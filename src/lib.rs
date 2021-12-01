#![forbid(unsafe_code)]
#![allow(dead_code)]

mod tests;
pub mod config;
pub mod btu_cron;

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

	use core::panic;

use chrono::{DateTime, Utc}; // See also: DateTime, Local, TimeZone
	use chrono::NaiveDateTime;
	use mysql::PooledConn;
	use mysql::prelude::Queryable;
	use redis::{self};
	use redis::{Commands, RedisError};

	use crate::btu_cron;
	use crate::config;
	use crate::pyrq;
	
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

		let mut redis_conn = pyrq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");

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

		let mut redis_conn = pyrq::get_redis_connection(app_config).expect("Unable to establish connection with RQ database server.");
		
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

	fn _from_tuple_to_rqscheduledjob(tuple: &(String, String)) -> RQScheduledJob {
		let timestamp: i64 = tuple.1.parse().unwrap();
		RQScheduledJob {
			job_id: tuple.0.clone(),
			start_datetime_unix: timestamp,
			start_datetime_utc: DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc)
		}
	}

	impl From<(String,String)> for RQScheduledJob {

		// TODO: Change to a Result with some kind of Error
		fn from(tuple: (String, String)) -> RQScheduledJob {
			_from_tuple_to_rqscheduledjob(&tuple)
		}
	}

	// NOTE:  This is the 'Newtype Pattern'
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
				dbg!("Converting from an empty tuple?");
				return VecRQScheduledJob::new();
			}
			dbg!("Step 1: Going from Vec<(String,String)> into VecRQScheduled Job.");
			dbg!(&vec_of_tuple);
			//let result: VecRQScheduledJob = VecRQScheduledJob::new();
			let result = vec_of_tuple.iter().map(_from_tuple_to_rqscheduledjob).collect();
			result
		}
	}

	pub fn rq_get_scheduled_tasks(app_config: &config::AppConfig) -> VecRQScheduledJob {
		/*
			Call RQ and request the list of values in "btu_scheduler:job_execution_times"
		*/
		let mut redis_conn = pyrq::get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
		let redis_result: Vec<(String, String)> = redis_conn.zscan(RQ_KEY_SCHEDULED_JOBS).unwrap().collect();

		println!("Answer from Redis: there are {} scheduled tasks.", redis_result.len());
		
		// return VecRQScheduledJob::new();

		let wrapped_result: VecRQScheduledJob = redis_result.into();
		wrapped_result		

		/*
		let mut result: Vec<RQScheduledJob> = Vec::new();
		for baz in redis_result.iter().collect::<Vec<&(String, String)>>() {
			let timestamp: i64 = baz.1.parse().unwrap();
			result.push(
				RQScheduledJob {
					job_id: baz.0.clone(),
					start_datetime_unix: timestamp,
					start_datetime_utc: DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp(timestamp, 0), Utc)
				}	
			);
		}
		*/

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

	read_task_from_sql()


	local_cron_to_utc_cron() {

	}

	rq_print_scheduled_tasks() {

	}

	cancel_scheduled_task(task_schedule_identifier) {

	}
	*/
}

pub mod pyrq {

	use crate::config::AppConfig;
	use redis::{Commands, RedisError};

	static RQ_JOB_PREFIX: &str = "rq:job";

	pub fn get_redis_connection(app_config: &AppConfig) -> Option<redis::Connection> {
		// Returns a Redis Connection, or None.
		let client: redis::Client = redis::Client::open(format!("redis://{}:{}/", app_config.rq_host, app_config.rq_port)).unwrap();
		if let Ok(result) = client.get_connection() {
			Some(result)
		}
		else {
			println!("Unable to establish a connection to Redis Server at host {}:{}",
				app_config.rq_host,
				app_config.rq_port
			);
			None
		}
	}

	pub fn read_job_by_id(app_config: &AppConfig, job_id: &str) -> () {

		let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
		let key: String = format!("{}:{}", RQ_JOB_PREFIX, job_id);
		let result: Result<String, RedisError> = redis_conn.hgetall(key);
		match result {
			Ok(foo) => {
				println!("Success.  Here's a string representation of what's in Redis: {}", foo);
			},
			Err(bar) => {
				println!("Error.  Here's what that error looks like: {}", bar);
			}
		}
		()
	}

}