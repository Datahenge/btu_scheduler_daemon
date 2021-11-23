## Why did I create this application?

My motivation was this:

I needed various [Frappe](https://github.com/frappe/frappe) and [ERPNext](https://github.com/frappe/erpnext) Python code to execute:

1. Periodically, based on a schedule.
2. Automatically, via worker threads on the Linux server.
3. With confidence, regardless of whether web servers are online, or not.
4. Allow website users to configure and update both Schedules and Tasks, in real time, and act on those changes immediately.
5. Capture the results of the scheduled tasks in a Log table, viewable on the website.

It was the latter 3 requirements that led me away from the out-of-the-box `Scheduled Job Types`, and into a custom application development.

#### What Code?  What Schedules?

The Python code to-be-executed, and the task schedules, are owned by my Frappe application: [*Background Tasks Unleashed*](https://github.com/Datahenge/btu)

  * Tasks and Schedules are stored in Frappe DocTypes (MySQL table).  These records are considering the single "Source of Truth".
  * Via their browser, website users can edit Tasks and Schedules.  They can also review the Logs, to see precisely what happened when those Tasks executed.
  * Tasks should be stored as Jobs in [Python RQ](https://python-rq.org/), which can be be processed by worker threads.

While helpful, the BTU application alone doesn't solve the entire problem.  The biggest gap was this:

*How do the BTU Tasks and Schedules get pushed into RQ, and remain synchronized?*

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
