/* Dev Notes:

  * No need to create a 'config' mod { } here, since we're in a separate physical file.
  * Do not import mysql like this: 'use mysql::*;'.  Doing so will override certain default types, like Error.

*/

use std::{fmt, fs};
use std::path::{Path, PathBuf};
use camino::Utf8PathBuf;

use chrono_tz::Tz;
use mysql::{Opts, Pool};
use serde::{Deserialize, Serialize};
use tracing::Level;
use tracing_subscriber::filter;

use crate::config::error::ConfigError;
use crate::logging::{LevelWrapper, LevelFilterWrapper};
use tracing::{trace, debug, info, warn, error, span};

static CONFIG_FILE_PATH: &'static str = "/etc/btu_scheduler/btu_scheduler.toml";

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

#[derive(Deserialize, Serialize)]
pub struct AppConfig {

	pub environment_name: Option<String>,
	pub full_refresh_internal_secs: u32,
	pub time_zone_string: String,
	pub tracing_level: LevelFilterWrapper,
	pub startup_without_database_connections: bool,

	pub email_address_from: Option<String>,
	pub email_host_name: Option<String>,
	pub email_host_port: Option<i16>,
	pub email_account_name: Option<String>,
	pub email_account_password: Option<String>,

	pub email_addresses: Option<Vec<String>>,
	pub email_on_level: Option<LevelWrapper>,  // A wrapper around Level, because the tracing crate doesn't implement Serialize and Deserialize.
	pub email_when_queuing: bool,
	mysql_user: String,
	mysql_password: String,
	mysql_host: String,
	mysql_port: Option<u32>,
	mysql_database: String,
	pub rq_host: String,
	pub rq_port: u32,
	pub scheduler_polling_interval: u64,
	pub socket_path: String,  // Dev Note: The level of effort to make this a PathBuf or Utf8PathBuf, and incorporate with MutexGuard: just too much!
	pub socket_file_group_owner: String,
	pub webserver_ip: String,
    pub webserver_port: u16,
	pub webserver_host_header: Option<String>,
    pub webserver_token: String
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

	pub fn new_from_toml_file(config_file_path: Option<&str>) -> Result<AppConfig, ConfigError> {

		// Read TOML file, and store values here in this configuration.
		let file_path: Utf8PathBuf;
		if config_file_path.is_some() {
			file_path = config_file_path.unwrap().into();
		}
		else {
			file_path = CONFIG_FILE_PATH.into();
		}

		if ! file_path.exists() {
			// Originally I intended to create a default configuration.  
			// But this requires elevating to root and restarting the app.  And either way, the user needs to manually key in
			// values for MySQL and Redis credentials.  So better to just print and exit.
			println!("\nError: Configuration file '{}' does not exist.", file_path);
			AppConfig::print_default_config_exit(&file_path);
		}

		let file_contents: String = fs::read_to_string(file_path)
			.expect("Something went wrong while reading the TOML file.");

		AppConfig::new_from_toml_string(&file_contents)
	}

	pub fn print_default_config_exit(file_path: &Utf8PathBuf) -> () {
		error!("\nError: No configuration file was found at path: {}", file_path);
		error!("You will need to create a configuration file manually.");
		error!("Below is an example of the file's contents:\n");
		let default_config = AppConfig {
			environment_name: Some("Development".to_string()),
			full_refresh_internal_secs: 180,
			time_zone_string: "UTC".to_string(),
			tracing_level: LevelFilterWrapper::new(filter::LevelFilter::INFO),
			startup_without_database_connections: false,
			email_address_from: None,
			email_host_name: None,
			email_host_port: None,
			email_account_name: None,
			email_account_password: None,
			email_addresses: None,
			email_on_level: Some(LevelWrapper::new(Level::DEBUG)),
			email_when_queuing: false,
			mysql_user: "root".to_string(),
			mysql_password: "foo".to_string(),
			mysql_host: "127.0.0.1".to_string(),
			mysql_port: Some(3306),
			mysql_database: "bar".to_string(),
			rq_host: "127.0.0.1".to_string(),
			rq_port: 11000,
			scheduler_polling_interval: 60,
			socket_path: "/tmp/btu_scheduler.sock".to_string(),
			socket_file_group_owner: "frappe_group".to_string(),
            webserver_ip: "127.0.0.1".to_string(),
            webserver_port: 8000,
			webserver_host_header: Some("mysubdomain.domain.com".to_string()),
            webserver_token: "token: abcd1234".to_string()
		};
		let toml_string = toml::to_string(&default_config).unwrap();
		warn!("{}", toml_string);
		std::process::exit(1);
	}

	pub fn tz(&self) -> Result<chrono_tz::Tz, chrono_tz::ParseError> {

		let _: Tz = match self.time_zone_string.parse() {
			Ok(v) => {
				return Ok(v);
			}
			Err(e) => {
				return Err(e)
			}
		};
	}

}

impl fmt::Display for AppConfig {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "BTU Application Configuration ({}):\n
* MySQL Username: {}
* MySQL Password: {}
* MySQL Host: {}.{:?}
* MySQL Database: {}
* Path to Socket File: {}
* RQ Host: {}
* RQ Port: {}
* Unix Domain Socket Path: {}
* Socket File Group Owner: {}
* Scheduler Polling Interval: {}
* Seconds Between Refresh: {}
* Web Server IP: {},
* Web Server Port: {},
* Web Server Host Header: {:?},
* Web Server Token: {},
",
			CONFIG_FILE_PATH,
			self.mysql_user,
			"********",
			self.mysql_host,
			self.mysql_port.unwrap_or(3306),
			self.mysql_database,
			self.socket_path,
			self.rq_host,
			self.rq_port,
			self.socket_path,
			self.socket_file_group_owner,
			self.scheduler_polling_interval,
			self.full_refresh_internal_secs,
			self.webserver_ip,
			self.webserver_port,
			self.webserver_host_header,
			self.webserver_token
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


// Brian:  Would be great to accomplish this, so I could store Tz inside of other structs.
//         However, implementing Serialize and Deserialize for Tz is beyond my capabilities at the moment.

/*
	pub struct MyTz ( chrono_tz::Tz );  // tuple struct: See article https://rust-unofficial.github.io/patterns/patterns/behavioural/newtype.html

	impl MyTz {
		pub fn new(tz: chrono_tz::Tz) -> MyTz {
			MyTz(tz)
		}
	}

	impl Serialize for MyTz {
		fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
			where S: Serializer
		{
			// 3 is the number of fields in the struct.
			let mut tup = serializer.serialize_tuple(1)?;
			tup.serialize_element(&self.0.to_string())?;  // Unsure if this is reasonable, but converting the TZ to a string seems the easiest approach to Serialization.
			tup.end()
		}
	}
	impl<'a> Deserialize<'a> for MyTz {
		fn deserialize<'de, D>(deserializer: D) -> Result<Self, D::Error>
			where D: Deserializer<'a>
		{
			deserializer.deserialize_string(MyTz::new(D))
		}
	}

*/
