// scheduler.rs

use std::collections::VecDeque;
use std::fmt;
use anyhow::anyhow as anyhow_macro;
use chrono::{DateTime, SecondsFormat, Utc}; // See also: DateTime, Local, TimeZone
use chrono::NaiveDateTime;
use redis::{self, Commands, RedisError};
use tracing::{trace, debug, info, warn, error, span, Level};

#[cfg(feature = "email-feat")]
use crate::email;
#[cfg(feature = "email-feat")]
use crate::email::{BTUEmail, make_email_body_preamble};

use crate::{btu_cron, config, rq};
use crate::task_schedule::{BtuTaskSchedule, read_btu_task_schedule};

// static RQ_SCHEDULER_NAMESPACE_PREFIX: &'static str = "rq:scheduler_instance:";
// static RQ_KEY_SCHEDULER: &'static str = "rq:scheduler";
// static RQ_KEY_SCHEDULER_LOCK: &'static str = "rq:scheduler_lock";
static RQ_KEY_SCHEDULED_TASKS: &'static str = "btu_scheduler:task_execution_times";


pub struct TSIK(String);

impl TSIK {

	pub fn task_schedule_id(&self) -> &str {
		self.0.split("|").collect::<Vec<&str>>()[0]
	}
	pub fn next_unix_datetime(&self) -> i64 {
		self.0.split("|").collect::<Vec<&str>>()[1].parse::<i64>().unwrap()
	}
}

impl From<String> for TSIK {
	fn from(any_string: String) -> TSIK {
		TSIK(any_string)
	}
}

impl From<&str> for TSIK {
	fn from(any_string: &str) -> TSIK {
		TSIK(any_string.to_owned())
	}
}

#[derive(Debug, PartialEq, Clone)]
pub struct RQScheduledTask {
	pub task_schedule_id: String,
	pub next_datetime_unix: i64,
	pub next_datetime_utc: DateTime<Utc>,
}

impl RQScheduledTask {

	pub fn to_tsik(&self) -> String {
		format!("{}|{}", self.task_schedule_id, self.next_datetime_unix)
	}		

	pub fn from_tsik(tsik: TSIK) -> RQScheduledTask {
 
		// Create a NaiveDateTime from the Unix timestamp
 		let next_naive = NaiveDateTime::from_timestamp_opt(tsik.next_unix_datetime(), 0).unwrap();
    	// Create a normal DateTime from the NaiveDateTime
 		let next_utc: DateTime<Utc> = DateTime::from_utc(next_naive, Utc);
		// Build a new struct
		RQScheduledTask {
			task_schedule_id: tsik.task_schedule_id().to_owned(),
			next_datetime_unix: tsik.next_unix_datetime(),
			next_datetime_utc: next_utc
		}
	}
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

impl From<(&str,&str)> for RQScheduledTask {
	// TODO: Change to a Result with some kind of Error
	fn from(tuple: (&str, &str)) -> RQScheduledTask {
		let owned_tuple = (tuple.0.to_owned(), tuple.1.to_owned());
		_from_tuple_to_rqscheduledtask(&owned_tuple).unwrap()
	}
}

fn _from_tuple_to_rqscheduledtask(tuple: &(String, String)) -> Result<RQScheduledTask, std::io::Error> {
	/* 
		The tuple argument consists of 2 Strings: JobId and Unix Timestamp.
		Using this information, we can build an RQScheduledTask struct.
		There is no reason to consume the tuple; so accepting it as a reference.
	*/
	let timestamp: i64 = tuple.1.parse::<i64>().unwrap();  // coerce the second String into an i64, using "turbofish" syntax
	let utc_datetime:  DateTime<Utc> = DateTime::<Utc>::from_utc(NaiveDateTime::from_timestamp_opt(timestamp, 0).unwrap(), Utc);
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
		
		1. This function's only caller is Thread #1.

		2. This function's concept was derived from the Python 'rq_scheduler' library.  In that library, the public
			entrypoint (from the website) was named a function 'cron()'.  That cron() function did a few things:

			* Created an RQ Job object in the Redis datbase.
			* Calculated the RQ Job's next execution time, in UTC.
			* Added a 'Z' key to Redis where the value of 'Score' is the next UTC Runtime, but expressed as a Unix Time.

				self.connection.zadd("rq:scheduler:scheduled_jobs", {job.id: to_unix(scheduled_time)})

		3. I am making a deliberate decision to -not- create an RQ Job at this time.  But instead, to create the RQ
			Job later, when it's time to actually run it.
			
			My reasoning is this: a Frappe web user might edit the definition of a Task between the time it was scheduled
			in RQ, and the time it actually executes.  This would make the RQ Job stale and invalid.  So anytime someone edits
			a BTU Task, I would have to rebuild all related Task Schedules.  Instead, by waiting until execution time, I only have
			to react to Schedule modfications in the Frappe web app; not Task modifications.

			The disadvantage: if the Frappe Web Server is not online and accepting REST API requests, when it's
			time to run a Task Schedule?  Then BTU Scheduler will fail: it cannot create a pickled RQ Job without the Frappe web server's APIs.

			Of course, if the Frappe web server is offline, that's usually an indication of a larger problem.  In which case, the
			BTU Task Schedule might fail anyway.  So overall, I think the benefits of waiting to create RQ Jobs outweighs the drawbacks.

		4. What if a race condition happens, where a newer Schedule arrives, before a previous Schedule has been sent to a Python RQ?
			A redis sorted set can only store the same key once.  If we make the Task Schedule ID the key, the newer "next date" will overwrite
			the previous one.

			To handle this, the Sorted Set "key" must be the concatentation of Task Schedule ID and Unix Time.
			I'm going to call this a TSIK (Task Scheduled Instance Key)
	*/

	/*
		Notice the line below: Only retrieving the 1st value from the result vector.  Later, it might be helpful to fetch
		multiple Next Execution Times, because of time zone shifts around Daylight Savings.
	*/
	let next_runtimes = task_schedule.next_runtimes(&None, &1);
	if next_runtimes.is_none() {
		return;
	}
	let rq_scheduled_task: RQScheduledTask = RQScheduledTask {
		task_schedule_id: task_schedule.id.to_owned(),
		next_datetime_unix: next_runtimes.as_ref().unwrap()[0].timestamp(),
		next_datetime_utc: next_runtimes.as_ref().unwrap()[0]
	};

	// Establish connection to Redis, and perform a ZADD
	// Someday, I can make this better, with RFC 3137, let-else statements
	// https://github.com/rust-lang/rust/issues/87335
	let redis_conn: Option<redis::Connection> = rq::get_redis_connection(app_config, false);
	if redis_conn.is_none() {
		return ();  // If cannot connect to Redis, do not panic the thread.  Instead, return an empty Vector.
	}

	let mut redis_conn: redis::Connection = redis_conn.unwrap();  // shadow the previous variable assignment
	let some_result: Result<std::primitive::u32, RedisError> = redis_conn.zadd(
		RQ_KEY_SCHEDULED_TASKS,
		rq_scheduled_task.to_tsik(),
		rq_scheduled_task.next_datetime_unix
	);

	match some_result {
		Ok(_result) => {
			trace!("Result from 'zadd' is Ok, with the following payload: {}", _result);
			// Developer Note: I believe a result of 1 means Redis wrote a new record.
			//                 A result of 0 means the record already existed, and no write was necessary.
			let message1: &str = &format!("Task Schedule ID {} is being monitored for future execution.", task_schedule.id);
			// If application configuration has a good Time Zone string, print Next Execution Time in local time...
			if let Ok(timezone) = app_config.tz() {
				let message2: &str = &format!("Next Execution Time ({}) for Task Schedule {} = {}", 
											timezone, 
											task_schedule.id, 
											rq_scheduled_task.next_datetime_utc.with_timezone(&timezone).to_rfc2822());	
				let message3: &str =  &format!("Next Execution Time (UTC) for Task Schedule {} = {}",
					                           task_schedule.id,
											   rq_scheduled_task.next_datetime_utc.to_rfc3339());
				debug!(message1, message2, message3);
			}
			else {
				// Otherwise, just print in UTC.	
				let message3: &str =  &format!("Next Execution Time (UTC) for Task Schedule {} = {}",
				                               task_schedule.id,
											   rq_scheduled_task.next_datetime_utc.to_rfc3339());
				debug!(message1, message3);
			}
		},
		Err(error) => {
			error!("Result from redis 'zadd' is Err, with the following payload: {}", error);
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

fn fetch_task_schedules_ready_for_rq(app_config: &config::AppConfig, sched_before_unix_time: i64) -> Vec<RQScheduledTask> {
	// Read the BTU section of RQ, and return the Jobs that are scheduled to execute before a specific Unix Timestamp.

	// Developer Notes: Some cleverness below, courtesy of 'rq-scheduler' project.  For this particular key,
	// the Z-score represents the Unix Timestamp the Job is supposed to execute on.
	// By fetching ALL values below a certain threshold (Timestamp), the program knows precisely which Task Schedules to enqueue...

	// rq_print_scheduled_tasks(&app_config);

	debug!("Reviewing the 'Next Execution Times' for each Task Schedule in Redis...");

	// Someday, I can make this better, with RFC 3137, let-else statements
	// https://github.com/rust-lang/rust/issues/87335
	let redis_conn: Option<redis::Connection> = rq::get_redis_connection(app_config, false);
	if redis_conn.is_none() {
		debug!("In lieu of a Redis Connection, returning an empty vector.");
		return Vec::new();  // If cannot connect to Redis, do not panic the thread.  Instead, return an empty Vector.
	}
	let mut redis_conn: redis::Connection = redis_conn.unwrap();

	// TODO: As per Redis 6.2.0, the command 'zrangebyscore' is considered deprecated.
	// Please prefer using the ZRANGE command with the BYSCORE argument in new code.
	let redis_result: Result<Vec<String>, redis::RedisError> = redis_conn.zrangebyscore(RQ_KEY_SCHEDULED_TASKS, 0, sched_before_unix_time);
	if redis_result.is_err() {
		return Vec::new();  // if nothing to enqueue, then return an empty Vector.
	}

	let zranges: Vec<String> = redis_result.unwrap();
	if zranges.len() > 0 {
		info!("Found {:?} Task Schedules that qualify for immediate execution.", zranges.len());
	}
	// The strings in the vector are a concatenation:  Task Schedule ID, pipe character, Unix Time.
	// Need to split off the trailing Unix Time, to obtain a list of Task Schedules.
	// NOTE: The syntax below is -very- "Rusty" (imo): maps the values returned by an iterator, using a closure function.
	let task_schedules_to_enqueue: Vec<RQScheduledTask> = zranges.iter().map(|x| -> RQScheduledTask {

		// each 'x' represents a pipe separated string.
		let tsik: TSIK = x.to_owned().into();
		RQScheduledTask::from_tsik(tsik)
	}).collect::<Vec<_>>();

	// Finally, return a Vector of Task Schedule identifiers:
	task_schedules_to_enqueue


}

/**
	 Examine the Next Execution Time for all scheduled RQ Jobs (this information is stored in RQ as a Unix timestamps)
	If the Next Execution Time is in the past?  Then place the RQ Job into the appropriate queue.  RQ and Workers take over from there.
*/

pub fn check_and_run_eligible_task_schedules(app_config: &config::AppConfig, internal_queue: &mut VecDeque<String>) {
	// Developer Note: This function is analgous to the 'rq-scheduler' Python function: 'Scheduler.enqueue_jobs()'
	let task_schedule_instances: Vec<RQScheduledTask> = fetch_task_schedules_ready_for_rq(app_config, Utc::now().timestamp());

	for task_schedule_instance in task_schedule_instances.iter() {
		info!("Time to make the donuts! (enqueuing Redis Job '{}' for immediate execution)", task_schedule_instance.task_schedule_id);
		match run_immediate_scheduled_task(app_config, task_schedule_instance, internal_queue) {
			Ok(_) => {
				#[cfg(feature = "email-feat")]  // Only compile this code when email feature is enabled:
				if app_config.email_when_queuing {
					// Send emails that mention the Task was enqueued.  This is useful for debugging or building confidence in the BTU.
					debug!("Attempting to send an email about this Task...");
					let body: String = format!("{}\n{}",
						make_email_body_preamble(app_config),
						format!("I am enqueuing BTU Task Schedule {} into a Python Redis Queue (RQ)", task_schedule_instance.task_schedule_id)
					);
					let email_result = crate::email::send_email(&app_config, "BTU is enqueuing a Task Schedule ", &body);  // don't lose ownership of the original
					debug!("SMTP Response: {:?}", email_result);
					if email_result.is_err() {
						error!("Error while attempting to send an email: {:?}", email_result.err().unwrap());
					}
				}
			},
			Err(err) => {
				error!("Error while attempting to run Task Schedule {} : {}", task_schedule_instance.task_schedule_id, err);
			}
		}
	}
}

pub fn run_immediate_scheduled_task(app_config: &config::AppConfig, 
									task_schedule_instance: &RQScheduledTask,
									internal_queue: &mut VecDeque<String>) -> Result<(), anyhow::Error> {

	// 0. First remove the Task from the Schedule (so it doesn't get executed twice)
	if rq::get_redis_connection(app_config, true).is_none() {
		warn!("Early exit from run_immediate_scheduled_task(); cannot establish a connection to Redis database.");
		return Ok(());  // If cannot connect to Redis, do not panic the thread.  Instead, return an empty Vector.
	}
	let mut redis_conn = rq::get_redis_connection(app_config, true).unwrap();
	let redis_result: u32 = redis_conn.zrem(RQ_KEY_SCHEDULED_TASKS, task_schedule_instance.to_tsik())?;
	
	if redis_result != 1 {
		error!("Unable to remove Task Schedule Instance using 'zrem'.  Response from Redis = {}", redis_result);
	}

	// 1. Read the MariaDB database to construct a BTU Task Schedule struct.
	let task_schedule = read_btu_task_schedule(app_config, &task_schedule_instance.task_schedule_id);
	if task_schedule.is_none() {
		return Err(anyhow_macro!("Unable to read Task Schedule from MariaDB database."));
	}
	let task_schedule: BtuTaskSchedule = task_schedule.unwrap();  // shadow original variable.

	// 2. Exit early if the Task Schedule is disabled (this should be a rare scenario, but definitely worth checking.)
	if task_schedule.enabled == 0 {
		warn!("Task Schedule {} is disabled in SQL database; BTU will neither execute nor re-queue.", task_schedule.id);
		return Err(anyhow_macro!("Task Schedule {} is disabled in SQL database; BTU will neither execute nor re-queue.", task_schedule.id));
	}
	// 3. Create an RQ Job from the BtuTask struct.
	let rq_job: rq::RQJob = task_schedule.to_rq_job(app_config)?;
	debug!("Created an RQJob struct: {}", rq_job);

	// 4. Save the new Job into Redis.
	rq_job.save_to_redis(app_config);

	// 5. Enqueue that job for immediate execution.
	match rq::enqueue_job_immediate(&app_config, &rq_job.job_key_short) {
		Ok(ok_message) => {
			info!("Successfully enqueued: {}", ok_message);
		}
		Err(err_message) => {
			error!("Error while attempting to queue job for execution: {}", err_message);
		}
	}
	/* 6. Recalculate the next Run Time.
		  Easy enough; just push the Task Schedule ID back into the -Internal- Queue! 
		  It will get processed automatically during the next thread cycle.
	*/
	internal_queue.push_back(task_schedule_instance.task_schedule_id.to_owned());
	Ok(())
}

pub fn rq_get_scheduled_tasks(app_config: &config::AppConfig) -> VecRQScheduledTask {
	/*
		Call RQ and request the list of values in "btu_scheduler:job_execution_times"
	*/

	// Someday, I can make this better, with RFC 3137, let-else statements
	// https://github.com/rust-lang/rust/issues/87335
	let redis_conn: Option<redis::Connection> = rq::get_redis_connection(app_config, false);
	if redis_conn.is_none() {
		debug!("In lieu of a Redis Connection, returning an empty vector.");
		return Vec::new().into();  // If cannot connect to Redis, do not panic the thread.  Instead, return an empty Vector.
	}

	let mut redis_conn: redis::Connection = redis_conn.unwrap();
	let redis_result: Vec<(String, String)> = redis_conn.zscan(RQ_KEY_SCHEDULED_TASKS).unwrap().collect();  // vector of tuple
	let number_results = redis_result.len();
	let wrapped_result: VecRQScheduledTask = redis_result.into();
	if number_results != wrapped_result.len() {
		println!("Unexpected Error: Number values in Redis: {}.  Number values in VecRQScheduledTask: {}", number_results, wrapped_result.len());
	}
	wrapped_result		
}

/**
	Remove a Task Schedule from the Redis database, to prevent it from executing in the future.
*/	
pub fn rq_cancel_scheduled_task(app_config: &config::AppConfig, task_schedule_id: &str) -> Result<String,String> {
	
	// As of changes made May 21st 2022, the members in the Ordered Set 'btu_scheduler:task_execution_times'
	// are not just Task Schedule ID's.  The Unix Time is a suffix.  Removing members now requires some "starts_with" logic.

	// First, list all the keys using 'zrange btu_scheduler:task_execution_times 0 -1'
	let mut redis_conn = rq::get_redis_connection(app_config, true).expect("Unable to establish a connection to Redis.");	
	let all_task_schedules: redis::RedisResult<Vec<String>> = redis_conn.zrange(RQ_KEY_SCHEDULED_TASKS, 0, -1);
	if all_task_schedules.is_err() {
		return Err(all_task_schedules.err().unwrap().to_string());
	}
	let mut removed: bool = false;

	for each_row in all_task_schedules.unwrap() {
		if each_row.starts_with(task_schedule_id) {
			let redis_result: redis::RedisResult<u64> = redis_conn.zrem(RQ_KEY_SCHEDULED_TASKS, each_row);
			if redis_result.is_err() {
				return Err(redis_result.err().unwrap().to_string());
			}
			if redis_result.unwrap() == 0 {
				removed = true;
			}
			
		}
		// info!("{}", each_row);
	}
	if removed {
		return Ok("Scheduled Task successfully removed from Redis Queue.".to_owned());			
	} else {
		return Ok("Scheduled Task not found in Redis Queue.".to_owned());				
	}
}

/**
	Prints upcoming Task Schedules using the configured Time Zone.
*/
pub fn rq_print_scheduled_tasks(app_config: &config::AppConfig, to_stdout: bool) {

	let tasks: VecRQScheduledTask = rq_get_scheduled_tasks(app_config);  // fetch all the scheduled tasks.
	let local_time_zone: chrono_tz::Tz = app_config.tz().unwrap();  // get the time zone from the Application Configuration.

	println!("There are {} BTU Tasks scheduled for automatic execution:", tasks.len());
	for result in tasks.sort_by_id().iter() {
		let next_datetime_local = result.next_datetime_utc.with_timezone(&local_time_zone);
		let message: &str = &format!("Task Schedule {schedule} is scheduled to occur later at {time}", schedule=result.task_schedule_id, time=next_datetime_local);
		if to_stdout {
			println!("    {}", message);
		}
		else {
			info!(message);	
		}
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
