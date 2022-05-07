#![allow(dead_code)]

/*

This module should stay focused on Python RQ features and functionality,
and not cross over into how the BTU works.

*/

use std::fmt;
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use redis::{Commands, RedisError};
use uuid::Uuid;
use tracing::{trace, debug, info, warn, error, span, Level};

use crate::config::AppConfig;

static RQ_JOB_PREFIX: &str = "rq:job";

#[derive(Debug)]
pub struct RQJob {
	pub job_key: String,
	pub job_key_short: String,
	created_at: DateTime<Utc>,
	pub data: Vec<u8>,
	pub description: String,
	ended_at: Option<String>,
	enqueued_at: Option<String>,
	exc_info: Option<String>,
	last_heartbeat: String,
	meta: Option<Vec<u8>>,
	origin: String,
	result_ttl: Option<String>,
	started_at: Option<String>,
	status: Option<String>,  // not initially populated
	pub timeout: u32,
	worker_name: String,
}

fn option_string_to_owned(element: &Option<String>) -> String {
	// Awkward, but makes for cleaner syntax in 'save_to_redis()' below.
	match element {
		Some(value) => {
			value.to_owned()
		},
		None => {
			"".to_owned()
		}
	}
}

impl RQJob {

	pub fn new_with_defaults() -> Self {

		// example: 11f83e81-83ea-4df2-aa7e-cd12d8dec779
		let uuid_string: String = Uuid::new_v4().to_hyphenated().to_string();
		RQJob {
			job_key: format!("{}:{}", RQ_JOB_PREFIX, uuid_string),  // str(uuid4())
			job_key_short: uuid_string,
			created_at: chrono::offset::Utc::now(),
			description: "".to_owned(),
			data: Vec::new(),
			ended_at: None,
			enqueued_at: None,  // not initially populated
			exc_info: None,
			last_heartbeat: chrono::offset::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
			meta: None,
			origin: "default".to_owned(),  // begin with the queue named 'default'
			result_ttl: None,
			started_at: None,
			status: None,
			timeout: 600,  // default of 600 seconds (5 minutes)
			worker_name: "".to_owned(),
		}
	}

	/// Save the RQ struct to the Redis database.
	pub fn save_to_redis(&self, app_config: &AppConfig) -> () {
		// This function was a lot more work than expected.  Even though I'm takig a reference to the struct,
		// I have to explicitely clone() all Strings.  And for Option<String>, explicitely as_ref()
		let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");

		let values: Vec<(&'static str, String)> =  vec![
			( "status", option_string_to_owned(&self.status) ),
			( "worker_name", self.worker_name.clone() ),
			( "ended_at", option_string_to_owned(&self.ended_at)),
			( "result_ttl", option_string_to_owned(&self.result_ttl) ),
			( "enqueued_at",  option_string_to_owned(&self.enqueued_at) ),
			( "last_heartbeat", self.last_heartbeat.clone() ),
			( "origin", self.origin.clone() ),
			( "description", self.description.clone() ),
			( "started_at",  option_string_to_owned(&self.started_at) ),
			( "created_at", utc_to_rq_string(self.created_at) ),
			( "timeout", self.timeout.to_string() )
		];

		// When using hset_multiple, the values must all be of the same Type.
		// In the case below, an Array of Tuples, where the Tuple is (&str, &String)
		let _: () = redis_conn.hset_multiple(&self.job_key, &values).expect("Failed to execute HSET.");
		let _: () = redis_conn.hset(&self.job_key, "data", &self.data).expect("failed to execute HSET");
		if self.meta.is_some() {
			let _: () = redis_conn.hset(&self.job_key, "meta", &self.meta.as_ref().unwrap()).expect("failed to execute HSET");
		}
	}
}


pub fn utc_to_rq_string(datetime_utc: DateTime<Utc>) -> String {
	// The format is VERY important.  If the UTC DateTime is not correctly formatted,
	// it will -crash- the Python RQ Worker.
	datetime_utc.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}


impl fmt::Display for RQJob {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {

		// This syntax helpfully ignores the leading whitespace on successive lines.
		let mut meta_length: usize = 0;
		if self.meta.is_some() {
			meta_length = self.meta.as_ref().unwrap().len()
		}
		write!(f,  "job_key: {}\n\
					job_key_short: {}\n\
					created_at: {}\n\
					data: <bytes> with length {}\n\
					description: {}\n\
					ended_at: {:?}\n\
					enqueued_at: {:?}\n\
					last_heartbeat: {}\n\
					origin: {}\n\
					meta: <bytes> with length {}\n\
					result_ttl: {:?}\n\
					started_at: {:?}\n\
					status: {:#?}\n\
					timeout: {}\n\
					worker_name: {}
			",
			self.job_key, self.job_key_short,  self.created_at, self.data.len(), 
			self.description, self.ended_at, self.enqueued_at,
			self.last_heartbeat, self.origin, meta_length, self.result_ttl,  
			self.started_at, self.status, self.timeout, self.worker_name
		)
	}
}


fn bytes_to_hex_string(bytes: &Vec<u8>) -> String {

	let strs: Vec<String> = bytes.iter()
									.map(|b| format!("{:02X}", b))
									.collect();
	strs.join(" ")
}


pub fn enqueue_job_immediate(app_config: &AppConfig, job_id: &str) -> Result<String, std::io::Error> {

	let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
	let job = read_job_by_id(app_config, job_id)?;

	// 1. Add the queue name to 'rq:queues'.
	let queue_key: String = format!("rq:queue:{}", job.origin);
	let some_result: Result<u32, RedisError> = redis_conn.sadd("rq:queues", &queue_key);
	if some_result.is_err() {
		return Err(std::io::Error::new(std::io::ErrorKind::Other, some_result.unwrap_err()));
	}

	// 2. Push the job onto the queue.
	// NOTE: The return value of 'rpush' is an integer, representing the length of the List, after the completion of the push operation.
	let push_result: Result<u32, RedisError> = redis_conn.rpush(&queue_key, job_id);
	match push_result {
		Ok(foo) => {
			return Ok(format!("Enqueued job '{}' for immediate execution. Length of list after 'rpush' operation: {}", job_id, foo))
		}
		Err(bar) => {
			return Err(std::io::Error::new(std::io::ErrorKind::Other, bar));
		}
	}
}


pub fn exists_job_by_id(app_config: &AppConfig, job_id: &str) -> bool {
	/*
		Given a potential RQ Job ID, return a boolean True if it exists in the RQ database.
	*/
	let key: String = format!("{}:{}", RQ_JOB_PREFIX, job_id);
	let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
	let result: Result<HashMap<String, Vec<u8>>, RedisError> =  redis_conn.hgetall(key);

	match result {
		Ok(rq_hashmap) => {
			if rq_hashmap.len() == 0 {
				warn!("Redis returned no results for Hashmap key {}", job_id);
				return false
			}
			true
		},
		Err(e) => {
			error!("{:?}", e);
			false
		}
	}
}


pub fn get_redis_connection(app_config: &AppConfig) -> Option<redis::Connection> {
	// Returns a Redis Connection, or None.
	let client: redis::Client = redis::Client::open(format!("redis://{}:{}/", app_config.rq_host, app_config.rq_port)).unwrap();
	if let Ok(result) = client.get_connection() {
		Some(result)
	}
	else {
		error!("Unable to establish a connection to Redis Server at host {}:{}",
			app_config.rq_host,
			app_config.rq_port
		);
		None
	}
}


pub fn get_all_job_ids(app_config: &AppConfig) -> Option<Vec<String>> {
	let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
	match redis_conn.keys("rq:job:*") {
		Ok(keys) => {
			Some(keys)
		},
		Err(_) => {
			None
		}
	}
}

/// Converting a Redis hashmap value into an owned Option String.
pub fn hashmap_value_to_optstring(hashmap: &HashMap<String, Vec<u8>>, key: &str) -> Option<String> {
	// NOTE: This function saves a ton of syntax in the library. 
	match hashmap.get(key) {
		Some(value) => {
			Some(String::from_utf8_lossy(value).to_string())
		},
		None => {
			None
		}
	}
}


pub fn hashmap_value_to_utcdatetime(hashmap: &HashMap<String, Vec<u8>>, key: &str) -> Option<DateTime<Utc>> {
	// NOTE: This function saves a ton of syntax in the library. 
	match hashmap_value_to_optstring(hashmap, key) {
		Some(value) => {
			match chrono::DateTime::parse_from_rfc3339(&value) {
				Ok(value) => {
					Some(value.into())  // this is perhaps too-implicit, but it's converting a DateTime<FixedOffset> to UTC.
				},
				Err(err) => {
					error!("Error while converting hashmap key '{}' to UTC DateTime: {}", key, err);
					None
				}
			}
		},
		None => {
			None
		}
	}
}


pub fn read_job_by_id(app_config: &AppConfig, job_id: &str) -> Result<RQJob, std::io::Error> {

	let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
	let key: String = format!("{}:{}", RQ_JOB_PREFIX, job_id);

	let result: Result<HashMap<String, Vec<u8>>, RedisError> =  redis_conn.hgetall(&key); // reference to avoid a Move.
	match result {
		Ok(rq_hashmap) => {

			if rq_hashmap.len() == 0 {
				let message: String = format!("Job with key '{}' does not exist in the RQ database.", key);
				return Err(std::io::Error::new(std::io::ErrorKind::Other, message));
			}

			// Kind of wonky: Asking if the length of the hashmap one of [11, 12, 13]?
			if ! vec![11, 12, 13, 14].contains(&rq_hashmap.len()) {
				let message: String = format!("Expected Redis to return a Hashmap with 11 to 13 keys, but found {} keys instead.",
				                              rq_hashmap.len());
				return Err(std::io::Error::new(std::io::ErrorKind::Other, message));											  
			}

			let my_job: RQJob = RQJob {
				job_key: key,
				job_key_short: job_id.to_string(),
				status: hashmap_value_to_optstring(&rq_hashmap, "status"),
				data: rq_hashmap.get("data").unwrap().to_owned(),
				exc_info: hashmap_value_to_optstring(&rq_hashmap, "exc_info"),
				ended_at: hashmap_value_to_optstring(&rq_hashmap, "ended_at"),
				result_ttl: hashmap_value_to_optstring(&rq_hashmap, "result_ttl"),
				enqueued_at: hashmap_value_to_optstring(&rq_hashmap, "enqueued_at"),
				last_heartbeat: String::from_utf8_lossy(rq_hashmap.get("last_heartbeat").unwrap()).to_string(),
				origin: String::from_utf8_lossy(rq_hashmap.get("origin").unwrap()).to_string(),
				description: String::from_utf8_lossy(rq_hashmap.get("description").unwrap()).to_string(),
				meta: match rq_hashmap.get("meta") {
					Some(value) => {
						Some(value.to_owned())
					},
					_ => {
						None
					}
				},
				started_at: hashmap_value_to_optstring(&rq_hashmap, "started_at"),
				created_at: hashmap_value_to_utcdatetime(&rq_hashmap, "created_at").unwrap(),
				timeout: match rq_hashmap.get("timeout") {
					Some(timeout_string) => {
						redis_value_to_u32(timeout_string).unwrap()
					},
					None => {
						600  // default value of 600 second timeout (5 minutes)
					}
				},			
				worker_name: String::from_utf8_lossy(rq_hashmap.get("worker_name").unwrap()).to_string(),
			};
			return Ok(my_job)
		},
		Err(bar) => {
			return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("Redis HGETALL returned an error like this: {}", bar)));
		}
	}
}


/// Converts a Redis byte string to a signed 32-bit integer.
pub fn redis_value_to_i32(redis_value: &Vec<u8>) -> Result<i32, &str> {
	/* 
		Redis does not have an Integer type; only Strings.  
	   	To create a Rust Integer, we transform Redis bytes to UTF-8 String, then String to integer.
	*/
	let num_as_string: String = String::from_utf8_lossy(redis_value).into_owned();
	
	if let Ok(num) = num_as_string.trim().parse::<i32>() {
		return Ok(num);
	} 
	else {
		return Err("Could not convert Redis string value into a signed 32-bit integer.");
	}
}

/// Converts a Redis byte string to a signed 32-bit integer.
pub fn redis_value_to_u32(redis_value: &Vec<u8>) -> Result<u32, &str> {
	/* 
		Redis does not have an Integer type; only Strings.  
	   	To create a Rust Integer, we transform Redis bytes to UTF-8 String, then String to integer.
	*/
	let num_as_string: String = String::from_utf8_lossy(redis_value).into_owned();
	
	if let Ok(num) = num_as_string.trim().parse::<u32>() {
		return Ok(num);
	} 
	else {
		return Err("Could not convert Redis string value into an unsigned 32-bit integer.");
	}
}
