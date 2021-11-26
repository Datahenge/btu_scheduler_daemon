// Dev Note: No need to create a 'config' mod { } here, since we're in a separate physical file.

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
		MissingConfigFile
	}
}

use std::{fmt, fs};
use std::path::{Path};
use serde::{Deserialize, Serialize};
// use mysql::*;			// WARNING: Do -not- import mysql like this.  It will override default types, like Error.
use mysql::{Opts, Pool};
use crate::config::error::ConfigError;


static CONFIG_FILE_PATH: &'static str = "/etc/btu_scheduler/btu_scheduler.toml";

#[derive(Deserialize, Serialize)]
pub struct AppConfig {
	pub full_refresh_internal_secs: u32,
	mysql_user: String,
	mysql_password: String,
	mysql_host: String,
	mysql_port: Option<u32>,
	mysql_database: String,
	pub rq_host: String,
	pub rq_port: u32,
	pub scheduler_polling_interval: u64,
	pub socket_path: String  // Dev Note:  The level of effort to make this a PathBuf or Utf8PathBuf, and incorporate with MutexGuard?  TOO MUCH!
}

impl AppConfig {

	pub fn new_from_toml_string(any_string: &str) -> Result<AppConfig, ConfigError> {
		/* 
			Dev Notes: Rust and toml achieve some fanciness below.
			1. The raw string is converted to a TOML struct.
			2. That TOML struct is mapped 1:1 with my struct AppConfig, and all elements are populated.
		
			One reason this is possible?  The TOML specification has the concepts of strings, integers, and nulls.  :)
		*/
		match toml::from_str(&any_string) {
			Ok(app_config) => {
				Ok(app_config)
			},
			Err(error) => {
				return Err(ConfigError::ConfigLoad { source: error });
			}
		}
	}

	pub fn new_from_toml_file() -> Result<AppConfig, ConfigError> {

		// Read TOML file, and store values here in this configuration.
		let file_path = Path::new(CONFIG_FILE_PATH);
		if ! file_path.exists() {
			// Originally I intended to create a default configuration.  
			// But this requires elevating to root and restarting the app.  And either way, the user needs to manually key in
			// values for MySQL and Redis credentials.  So better to just print and exit.
			AppConfig::print_default_config_exit();
		}

		let file_contents: String = fs::read_to_string(file_path)
			.expect("Something went wrong while reading the TOML file.");
		// println!("Here are the contents of the TOML configuration file: {}", file_contents);

		let result = AppConfig::new_from_toml_string(&file_contents);
		result
		// println!("{}", config);  // uses the Display trait defined below.
	}

	pub fn print_default_config_exit() -> () {
		println!("\nError: No configuration file was found at path: {}", CONFIG_FILE_PATH);
		println!("You will need to create a configuration file manually.");
		println!("Below is an example of the file's contents:\n");
		let default_config = AppConfig {
			full_refresh_internal_secs: 180,
			mysql_user: "root".to_string(),
			mysql_password: "foo".to_string(),
			mysql_host: "127.0.0.1".to_string(),
			mysql_port: Some(3306),
			mysql_database: "bar".to_string(),
			rq_host: "127.0.0.1".to_string(),
			rq_port: 11000,
			scheduler_polling_interval: 60,
			socket_path: "/tmp/btu_scheduler.sock".to_string()
		};
		let toml_string = toml::to_string(&default_config).unwrap();
		println!("{}", toml_string);
		std::process::exit(1);
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
			self.full_refresh_internal_secs,
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
