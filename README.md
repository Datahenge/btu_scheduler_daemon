## Rusty PyRQ Scheduler

### Purpose

The purpose of this program is to create a Linux daemon that:

1. Initializes scheduled tasks (*for now, Python functions*) into a Redis Queue.
2. Listens for schedule changes over a Unix Domain Socket.
3. Periodically rereads the schedule from the 'system of record' (e.g. every 15 minutes)

### Why?

I need various ERPNext Python code to execute, on a schedule, via worker threads.
An ideal solution for this is [Python RQ](https://python-rq.org/).

#### What Code and What Schedule?
I am using my own Frappe application: "*Background Tasks Unleashed*"

  * The Task Code and Schedule are stored in an ERPNext DocType (MySQL table).  This table is the source of Truth.
  * Via the web browser, ERPNext users can enabled, disabled, or modify Task Schedules.

### Why not use RQ-Scheduler?

Initially, the third party [RQ Scheduler](https://github.com/rq/rq-scheduler) seems to be an ideal solution.

However, assume you reboot your Linux server.  First, you launch RQ.  Next, you launch RQ Scheduler.  What happens?

*Nothing at all.*

The problem with RQ Scheduler is that it's an entirely **passive** application:

* RQ Scheduler needs "*something*" to feed it the initial schedule data, which it writes to RQ.
* RQ Scheduler needs "*something*" to continuously communicate with it: creating, updating, and deleting schedules.

So, what is that "*something*"?

### Automation Options

#### 1. Modify the Frappe Framework
I could try hacking the Frappe core library, and teach its Web Server to initialize RQ Scheduler on boot. 

##### Challenges:

1. To make the BTU App independent and friendly, it must run on an **unmodified** Frappe Framework.
2. Editing the Frappe Framework to do things on boot?  Easier said than done.  The web server can have *multiple* Gunicorn Workers.  But the RQ only needs a *one-time* initialization.
   1. What about `hooks.py`?  Writing *anything* in that file is problematic.  A `hooks.py` might be processed *hundreds* of times an hour by the Frappe framework.  It's a very messy feature.
3. I want my BTU Tasks to execute on-schedule, *regardless* of whether the ERPNext Web Server is running.

#### 2. Modify RQ-Scheduler.
What if I forked the Python RQ Scheduler, and created an alternate version?

1. On startup, teach it to read and synchronize the initial schedules from the BTU SQL tables.
2. Teach it to periodically re-read the schedules from MySQL ever N minutes.
3. Stick with however it's currently handling Inter-process Communication (IPC).

##### Challenges:

* Python packaging and deployment sucks.  My fellow ERPNext enthusiasts can install BTU.  But asking them to install and daemonize another Python package?  Feels like that's full of pitfalls.
* Python daemons can confuse users, if you want them to leverage Virtual Environments (*and you probably do/should*)
* Python is great for scripting.  But time after time, I've found it lacking when it comes to hardened, industrial strength solutions.

#### 3. Write a better daemon from scratch.
I'm going with this option.  Once I've built this Rusty daemon, it will scale into other projects and requirements.


### Introducing: A Rusty 'PyRQ Scheduler'

Repeating my Purpose from above.  I want to write a daemon that:

1. Initializes scheduled tasks into a Python Redis Queue.
2. Listen for schedule changes over a Unix Domain Socket.
3. Periodically rebuilds the schedule from scratch (e.g. Every Hour)

### Additional Features

* Ships with a companion CLI application, that you can use to ask the daemon about its current status.
* Reads Task Schedules:
  * from MySQL database tables.
  * JSON and TOML files.
* Print activity to:
  * standard output.
  * log files.

#### See also:
https://github.com/Couragium/rsmq-async-rs


# License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.
