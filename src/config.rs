// Dev Note: No need to create another mod { } here, since we're in a separate physical file.

mod error {

	// Dev Note: Using the 'thiserror' crate to make for better escalation and casting of Err types.
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

use std::{fmt, fs};
use std::path::Path;
use serde::Deserialize;  	// Also there is Serialize
// use mysql::*;			// WARNING: Do -not- import mysql like this.  It will override default types, like Error.
use mysql::{Opts, Pool};
use crate::config::error::ConfigError;

#[derive(Deserialize)]
pub struct AppConfig {
	pub max_seconds_between_updates: u32,
	mysql_user: String,
	mysql_password: String,
	mysql_host: String,
	mysql_port: Option<u32>,
	mysql_database: String,
	pub rq_host: String,
	pub rq_port: u32,
	pub scheduler_polling_interval: u64
}

impl AppConfig {
	pub fn new_from_toml_string(any_string: &str) -> Result<AppConfig, ConfigError> {
		// Dev Notes: Rust + toml accomplish some fancy work here.  First, the raw string is converted to a TOML object.
		// Next, that TOML object is mapped 1:1 with the struct, and all elemnets are populated.
		// One reason this is possible?  The TOML specification has the concepts of strings, integers, and nulls.  :)
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
		let file_path = Path::new("/etc/btu_scheduler/.btu_scheduler.toml");
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
