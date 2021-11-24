## Why did I create this application?

My motivation was this:

I needed various [Frappe](https://github.com/frappe/frappe) and [ERPNext](https://github.com/frappe/erpnext) Python code to execute:

1. Periodically, based on a schedule.
2. Automatically, via worker threads on the Linux server.
3. With Confidence.  Regardless of whether web servers are online, or not.
4. Allow website users to configure and update both Schedules and Tasks in real time.  And be confident the Scheduler makes the changes immediately.
5. Capture *all* the results of a Scheduled Tasks in a human-readble Log table, viewable on the Frappe app's website.

It was the latter 3 requirements that convinced me the out-of-the-box `Scheduled Job Types` (Frapp v13) would never work.  I needed to write my own applications.

### What Code?  What Schedules?

The Python code to-be-executed (Tasks), and the cron schedules (Task Schedules), are owned by my Frappe application: [*Background Tasks Unleashed*](https://github.com/Datahenge/btu).

Summary:
  * Tasks and Schedules are stored in Frappe DocTypes (with persistent MySQL tables).  I consider these records to be the ["The Single Source of Truth"](https://en.wikipedia.org/wiki/Single_Source_of_Truth).
  * Via their browser, website users can edit Tasks and Schedules.  They can also review execution Logs (or receive them automatically via email) to understand precisely what happened when a Tasks executed.
  * Tasks and Schedules combine to become Jobs in [Python RQ](https://python-rq.org/), which can be be processed by worker threads.

While helpful, the BTU application alone doesn't solve the entire problem.  The biggest gap was this:

  "***How** do these BTU Tasks and Schedules get pushed into RQ, remain on-schedule, and synchronize with changes made via the website?"*

### Why not RQ-Scheduler?

Initially, the third party [RQ Scheduler](https://github.com/rq/rq-scheduler) application was a satisfactory solution.  However, it had a few gaps.

#### Problem #1:  Automatation and Persistency

Assume you reboot your Linux server.  Whether via [Supervisor](http://supervisord.org/) or [systemd](https://en.wikipedia.org/wiki/Systemd), various applications are launched: MySQL, Redis, the Frappe Web Server, Python RQ, and the RQ Scheduler.  But what happens to those scheduled Tasks?

*Probably nothing at all.*

The challenge with RQ Scheduler is that it's mostly a **passive** application:

* RQ Scheduler needs "*something*" to feed it the initial schedule data, which it writes to RQ.
* RQ Scheduler needs "*something*" to continuously communicate with it: creating, updating, and deleting schedules.

Even if we assume the data in Redis Queue was persisted data to an RDB file.  Are we *certain* it's 100% synchronized with the current values in the SQL table `tabBTU Task Schedule`?  Probably not.

Yet when it comes to ERP automation, we could *really* use some guarantees or safety nets.  One of the fundamental reasons for automation is taking a more hands-off approach.

So how can we be confident that Tasks are *always* being loaded, reloaded, scheduled, and executed?

#### Problem #2:  Cron expressed as Coordinated Universal Time (UTC)
RQ Scheduler and RQ expect that `cron` expressions are written per UTC.  From a system perspective, this is fantastic.  UTC is a very reliable way of both storing datetime values.  But also running them at precisely the correct moment.

However, from a user perspective...writing cron in UTC is awful.

Assume you have a Schedule that must execute at 7:00PM local time, the entire year.  To accomplish this you must:

1.  Initially write your Schedules with 1 cron expression, based on the delta between local time and UTC.
2.  You set a reminder to login the morning after Daylight Savings Time begins.
3.  You adjust your Schedule to a *different* cron expression, based on the **new** delta betwen your local time, and UTC.

Repeat this process for *every other Schedule you have*.

😬

A better idea is this:

* Users write their `cron` expressions against a local Time Zone.
* Each of those `cron` expressions evaluates into an array of *multiple* UTC cron expressions:
  * Before DST
  * After DST
  * (many more possibilities)
* The scheduler treats the 1 User Schedule as being **many** system-level Queue Schedules, for the same Task.

This would work great.  But the code to perform this has to be written *somewhere* ...

### Automation Options

#### 1. Modify the Frappe Framework
I could try hacking the Frappe core library, and teach its Web Server to initialize RQ Scheduler on boot. 

##### Challenges:

1. To make the BTU App independent and user-friendly, it should run on an **unmodified**, out-of-the-box Frappe Framework.  Not one of my forks.
2. Editing the Frappe Framework to do things on boot?  Easier said than done.  The web server can have *multiple* Gunicorn Workers.  But the RQ only needs a *one-time* initialization.
3. What about `hooks.py`?  Well, writing *anything* that file is problematic.  Each `hooks.py` might be processed *hundreds* of times an hour by the Frappe framework.  It's very unpredictable.  And we only want the Task Schedules synchronized *once*.
4. I'd like to be confident that Tasks are running on-schedule, *regardless* of whether the ERPNext Web Servers are running.

#### 2. Modify RQ-Scheduler.
What if I forked the Python RQ Scheduler, and created an alternate version?

1. On startup, teach it to read and synchronize the initial schedules from the BTU SQL tables.
2. Teach it to periodically re-read the schedules from MySQL ever N minutes.
3. Stick with however it's currently handling Inter-process Communication (IPC).

##### Challenges:

* Python packaging and deployment sucks (imho).  I'm confident my fellow ERPNext enthusiasts can install the Frappe BTU application.  But asking them to *also* install and daemonize another Python package?  That feels like that's full of pitfalls.
* Python daemons can confuse users, if you want them to leverage Virtual Environments (*and you probably do/should*)
* Python is great for scripting.  But time after time, I've found it lacking when it comes to hardened, industrial strength solutions.

#### 3. Write a better daemon from scratch.
I'm went this with this option.  Write a stable and safe Linux application using the Rust Programming Language, designed to be used as a daaemon.

### Introducing: BTU Scheduler Daemon.

Repeating my Purpose from above.  I'm creating a daemon that:

1. Initializes scheduled tasks into a Python Redis Queue.
2. Listen for schedule changes over a Unix Domain Socket.
3. Periodically rebuilds the schedule from scratch (e.g. N Minutes)
