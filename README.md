## BTU Scheduler Daemon

### Purpose

The purpose of this program is to create a background daemon that:

1. Stores scheduled tasks (*for now, Python functions living in Frappe and ERPNext*) as RQ Jobs in a Redis queue database.
2. Listens on a Unix Domain Socket for schedule updates from web clients.
3. Periodically (e.g. every 15 minutes) performs a "full-refresh" of the entire BTU Task Schedule data into RQ.
4. Very importantly, **enqueues** RQ jobs into the correct queues, at the correct times, based on the Schedules.  (RQ is pretty boring without something to actually populate its queues; this is one of those populators)

### Why did I make this?
Read [here](WHY.md) for more about why I needed to create this application.

### Prerequisites

* Linux 64bit operating system.  I tested this with Debian 11 Bullseye.
* This daemon isn't particularly useful without its companion Frappe application: [Background Tasks Unleashed (BTU)](https://github.com/Datahenge/btu)

### Installation
This scheduler is *not* a Python application like BTU.  It's a 64-bit Linux binary executable (created using [The Rust Programming Language](https://www.rust-lang.org/)).

1. Download the latest version from [Releases](https://github.com/Datahenge/btu_scheduler_daemon/releases).
2. Save this executable somewhere on your Frappe web server (*typical locations for third-party Linux programs are `/usr/local/bin`*)

### Configuration
Regardless of where you save the executable, you need to save your configuration file in a known location:
```
/etc/btu_scheduler/.btu_scheduler.toml
```

**Note**: This is a hidden file (notice the leading '.' in front of the file name).  I don't believe in security through obfuscation; I may change my mind about this convention.

```toml
# This is the TOML configuration file for the BTU Scheduler Daemon
name = "BTU Schedule Daemon"
max_seconds_between_updates = 90
mysql_user = "root"
mysql_password = "some_password"
mysql_host = "localhost"
mysql_port = 3313
mysql_database = "foo"
```

### Usage
#### Testing
To test the application, you probably want to intially run directly from a shell:
```
./btu_scheduler_daemon
```

To exit, just `CTRL+C`

#### Production or Live environments
For automatic startup, I recommend creating a **systemd** [service unit file](https://linuxconfig.org/how-to-create-systemd-service-unit-in-linux): `/etc/systemd/system/btu_scheduler.service`
```
[Unit]
Description=BTU Scheduler
After=network.target

[Service]
ExecStart=/path_to_file/btu_scheduler_daemon

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
