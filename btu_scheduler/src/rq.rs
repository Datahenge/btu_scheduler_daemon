use std::fmt;
use redis::{Commands, RedisError};
use crate::config::AppConfig;

static RQ_JOB_PREFIX: &str = "rq:job";

#[derive(Debug)]
pub struct RQJob {
	status: String,
	worker_name: String,
	ended_at: String,
	result_ttl: String,
	enqueued_at: String,
	last_heartbeat: String,
	origin: String,
	description: String,
	meta: Vec<u8>,
	started_at: String,
	created_at: String,
	timeout: i32,
	data: Vec<u8>
}


impl fmt::Display for RQJob {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {

		// This syntax helpfully ignores the leading whitespace on successive lines.
		write!(f, "status: {}\n\
					worker_name: {}\n\
					ended_at: {}\n\
					result_ttl: {}\n\
					enqueued_at: {}\n\
					last_heartbeat: {}\n\
					origin: {}\n\
					description: {}\n\
					meta: <bytes> with length {}\n\
					started_at: {}\n\
					created_at: {}\n\
					timeout: {}\n\
					data: <bytes> with length {}",
			self.status, self.worker_name, self.ended_at, self.result_ttl,
			self.enqueued_at, self.last_heartbeat, self.origin, self.description,
			self.meta.len(),
			self.started_at, self.created_at, self.timeout, self.data.len()
		)
	}
}


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
	//let result: Result<Vec<(String,String)>, RedisError> = redis_conn.hgetall(key);

	use std::collections::HashMap;
	let result: Result<HashMap<String, Vec<u8>>, RedisError> =  redis_conn.hgetall(key);
	match result {
		Ok(rq_hashmap) => {

			if rq_hashmap.len() == 0 {
				println!("Redis returned no results for Hashmap key {}", job_id);
				return ()
			}
			if rq_hashmap.len() != 13 {
				panic!("Expected Redis to return a Hashmap with 13 keys, but found {} keys instead.", rq_hashmap.len());
			}

			let my_job: RQJob = RQJob {
				status: String::from_utf8_lossy(rq_hashmap.get("status").unwrap()).to_string(),
				worker_name: String::from_utf8_lossy(rq_hashmap.get("worker_name").unwrap()).to_string(),
				ended_at: String::from_utf8_lossy(rq_hashmap.get("ended_at").unwrap()).to_string(),
				result_ttl: String::from_utf8_lossy(rq_hashmap.get("result_ttl").unwrap()).to_string(),
				enqueued_at: String::from_utf8_lossy(rq_hashmap.get("enqueued_at").unwrap()).to_string(),
				last_heartbeat: String::from_utf8_lossy(rq_hashmap.get("last_heartbeat").unwrap()).to_string(),
				origin: String::from_utf8_lossy(rq_hashmap.get("origin").unwrap()).to_string(),
				description: String::from_utf8_lossy(rq_hashmap.get("description").unwrap()).to_string(),
				meta: rq_hashmap.get("meta").unwrap().to_owned(),
				started_at: String::from_utf8_lossy(rq_hashmap.get("started_at").unwrap()).to_string(),
				created_at:String::from_utf8_lossy(rq_hashmap.get("created_at").unwrap()).to_string(),
				timeout: crate::rq::redis_value_to_i32(rq_hashmap.get("timeout").unwrap()).unwrap(),
				data: rq_hashmap.get("data").unwrap().to_owned()
			};

			println!("\n{}", my_job);
		},
		Err(bar) => {
			println!("Redis HGETALL returned an error like this: {}", bar);
		}
	}
	()
}


pub fn enqueue_job_immediate(app_config: &AppConfig, job_id: &str) -> () {

	let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
	let key: &'static str = "rq:queues";
	let member: String = format!("rq:queue:{}", job_id);
	
	let some_result: Result<String, RedisError> = redis_conn.sadd(key, member);
	println!("Value of 'some_result' = {:?}", some_result);
	println!("Enqueued job {} for immediate execution", job_id);
	()
}


pub fn exists_job_by_id(app_config: &AppConfig, job_id: &str) -> () {
	let key: String = format!("{}:{}", RQ_JOB_PREFIX, job_id);
	//let result: Result<Vec<(String,String)>, RedisError> = redis_conn.hgetall(key);

	use std::collections::HashMap;
	let mut redis_conn = get_redis_connection(app_config).expect("Unable to establish a connection to Redis.");
	let result: Result<HashMap<String, Vec<u8>>, RedisError> =  redis_conn.hgetall(key);
	match result {
		Ok(rq_hashmap) => {
			if rq_hashmap.len() == 0 {
				println!("Redis returned no results for Hashmap key {}", job_id);
				return ()
			}
		},
		Err(e) => {
			println!("{:?}", e);
		}
	}
}


pub fn redis_value_to_i32(redis_value: &Vec<u8>) -> Result<i32, &str> {

	let num_as_string: String = String::from_utf8_lossy(redis_value).into_owned();
	let num : i32 = num_as_string.trim().parse::<i32>().expect("Could not convert Redis string value into an Integer.");
	Ok(num)
}


pub fn bytes_to_hex_string(bytes: &Vec<u8>) -> String {

	let strs: Vec<String> = bytes.iter()
									.map(|b| format!("{:02X}", b))
									.collect();
	strs.join(" ")
}
