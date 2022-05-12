## BTU Scheduler

### Purpose

The purpose of this program is to create a background daemon that:

1. Stores scheduled tasks (*for now, Python functions living in Frappe and ERPNext*) as RQ Jobs in a Redis queue database.
2. Listens on a Unix Domain Socket for schedule updates sent from web servers (*and thus, indirectly from web clients*)
3. Periodically (e.g. every 15 minutes) performs a "full-refresh" of the entire BTU Task Schedule data into RQ.
4. Very importantly, **enqueues** RQ jobs into the correct queues, at the correct times, based on the Schedules.  (*Redis Queue would be pretty boring without something to populate its queues; this is one such thing.*)

### Official Documentation
The [Official Documentation](https://datahenge.github.io/btu_scheduler_daemon/) is hosted on a GitHub Pages Site.  All the markdown for that site is right here in this repository, in the `docs` directory.

### Why did I make this?
Read [here](docs/why_the_btu.md) for more about why I needed to create this application.

### Prerequisites

* Linux 64-bit operating system.  I tested this with Debian 11 Bullseye.
* A companion Frappe application: [Background Tasks Unleashed (BTU)](https://github.com/Datahenge/btu)

(*Note to Frappe Framework users: The BTU Scheduler daemon and CLI are -not- Python applications.  They are native Linux applications: 64-bit binary executables.  The source code was written in [The Rust Programming Language](https://www.rust-lang.org/).  This application coexists with the Frappe web server)*

### Installation
1. Download the latest version from [Releases](https://github.com/Datahenge/btu_scheduler_daemon/releases).  There are 2 binary applications:

  * `btu-daemon`:  Background daemon that interacts with Frappe BTU and [Python RQ](https://python-rq.org/).
  * `btu`:  Command line interface for interacting with the daemon and RQ database.

**NOTE/WARNING**:
Due to dependencies on C libraries, the is a **separate binary** for OS based on Debian 10, versus OS based on Debian 11.\
I am indicating in Releases which is which.  Make sure you download the correct binaries for your target environment.

2. Save the executables somewhere on your Frappe web server (*typical locations for third-party Linux programs are `/usr/local/bin`*)
3. Make sure the executables are on your Path, or make symlinks to them.

### Configuration
Regardless of where you save the executables, you must create and maintain a TOML configuration file here:
```
/etc/btu_scheduler/btu_scheduler.toml
```

Below is a sample of what this configuration file should look like.  You **must** edit this file, and enter your own environment's credentials and information.

```toml
name = "BTU Scheduler Daemon"
environment_name = "DEV"
full_refresh_internal_secs = 900
scheduler_polling_interval=60
time_zone_string="America/Los_Angeles"
tracing_level="INFO"

# Email Setup
email_address_from = "testing@datahenge.com"
email_host_name = "asmtp.mail.my_email_provider.com"
email_host_port = 587
email_account_name = "testing@datahenge.com"
email_account_password  = "my_smtp_email_password"

# Email Features
email_addresses = [ "brian@datahenge.com" ]
email_on_level="INFO"
email_when_queuing=true

# MySQL
mysql_user = "root"
mysql_password = "password_for_mysql_database"
mysql_host = "localhost"
mysql_port = 3306
mysql_database = "my_erpnext_db_name"

# RQ
rq_host = "127.0.0.1"
rq_port = 11000
socket_path = "/tmp/btu_scheduler.sock"
socket_file_group_owner = "erpnext_group"
webserver_ip = "127.0.0.1"
webserver_port = 8000
webserver_token = "token abcdef123456789:abcdef123456789"
```

* The `mysql_` keys are for your Frappe/ERPNext MariaDB database.
* The `rq_` keys are for your Redis Queue database.
* The `socket_path` is for the BTU background daemon.  I recommend just using the default value shown above.
* The `webserver_` keys are how BTU cannot to your ERPNext web server.  The `webserver_token` is the token for the ERPNext user that will act as a "service account" for BTU.

### Usage

#### Testing
To test the application, you may want to begin by running manually from a shell:
```
/usr/local/bin/btu_scheduler_daemon
# or
./btu_scheduler_daemon
```

The program runs indefinitely (unless it encounters a fatal error)\
To exit manually, use the keys `CTRL+C`

#### Production or Live environments
For automatic startup, I recommend creating a **systemd** [service unit file](https://linuxconfig.org/how-to-create-systemd-service-unit-in-linux): `/etc/systemd/system/btu_scheduler.service`
```
[Unit]
Description=BTU Scheduler
After=network.target

[Service]
ExecStart=/usr/local/bin/btu_scheduler_daemon

[Install]
WantedBy=multi-user.target
```

### TODO:
The following are some ideas I'm still working on:

* A companion CLI application you can use to ask the daemon about its current status.
* Reads Task Schedules not only from Frappe DocType `BTU Task Schedule`, but optionally from JSON or TOML files.
* Print activity to either standard output, or a log file.  The latter can be achieved through systemd service units.

#### See also:
https://github.com/Couragium/rsmq-async-rs


### License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.
