
use cron::Schedule;
use chrono::{DateTime, NaiveDateTime, TimeZone, Utc}; // See also: Local, TimeZone
use chrono_tz::Tz;
use std::str::FromStr;

use crate::error::CronError;

/* The Many Troubles of Cron

	1. The 'cron' crate only thinks about cron expressions as being written for UTC time.
	2. The 'cron' crate works with a 7-element cron expression.  Most Unix systems only recognize 5-elements.
	3. The 'cron' crate believes that Sunday is day 1 and Saturday is day 7.  This is contrary to other software (Sunday=0, Saturday=6)
	4. How do you convert from a TZ-aware cron expression to a UTC cron expression?
*/

pub fn cron_str_to_cron_str7 (cron_expression_string: &str) -> Result<String, CronError> {
	/*
	Given a cron string of N elements, transform into a cron string of 7 elements.

	Reasoning: There is no universal standard for cron strings.  They could contain 5-7 elements.  However
				the 'cron' library expects 7 elements.  This function pads any missing elements.
	*/

	let iter = cron_expression_string.trim().split_whitespace();
	let vec: Vec<&str> = iter.collect::<Vec<&str>>();

	match vec.len() {
		5 =>  {
			// Prefix with '0' for seconds, and suffix with '*' for years.
			return Ok(format!("0 {} *", cron_expression_string));
		},
		6 => {
			// Assume we're dealing with a cron(5) plus Year.  So prefix '0' for seconds.
			return Ok(format!("0 {}", cron_expression_string));
		},	
		7 => {
			// Cron string already has 7 elements, so pass it back.
			return Ok(cron_expression_string.to_owned())
		},
		_ => {
			return Err(CronError::WrongQtyOfElements { found: vec.len()});
		}				
	}
}


pub fn local_cron_to_utc_datetimes(cron_expression_string: &str, cron_timezone: Tz, number_of_results: usize) -> Result<Vec<DateTime<Utc>>, CronError> {
	/*
		Based on a cron string, what is the next, scheduled Datetime?
		Documentation: https://docs.rs/cron/0.9.0/cron

		Note: The Rust 'cron' library expects a 7-element cron string.  Where the additional elements are:
			--> Seconds
				Minutes
				Hours
				Day of Month
				Month
				Day of Week
			--> Years
	*/

	/* NOTE 1:  This is a VERY simplistic implementation of a valid list of UTC DateTimes.
	            What's actually required is something that handles Daylight Savings and time shifts.
				But it's good enough for today.
	*/

	/* NOTE 2:  Rather than returning a Vector of UTC Datetimes, it would be -better- to return an Iterator.
				I don't know how to do that in Rust (yet).  One step at a time.
	*/
	
	let cron7_expression = cron_str_to_cron_str7(cron_expression_string)?;
	let schedule = Schedule::from_str(&cron7_expression).unwrap();

	let mut virtual_datetimes: Vec<DateTime<Utc>> = Vec::new();

	/* The initial results below will be UTC datetimes.  Because that's what "schedule" is producing.
	 I'm going to work around that:
		1. Strip their time zone component, to make them naive datetimes.
		2. Then make them Local Times using the timezone supplied to this function.
		3. And *then* convert them to UTC.

		This will completely shatter during Daylight Savings...but it's a 98% okay function for today.
	*/
	for utc_datetime in schedule.upcoming(Utc).take(number_of_results) {
    	// dbg!("Next Loop", utc_datetime);
		let naive_datetime: NaiveDateTime = NaiveDateTime::from_timestamp(utc_datetime.timestamp(), 0);
		// dbg!(naive_datetime);
		let tz_aware = cron_timezone.from_local_datetime(&naive_datetime).unwrap();
		// dbg!(tz_aware);
		let new_utc_datetime: DateTime<Utc> = DateTime::<Utc>::from_utc(tz_aware.naive_utc(), Utc);
		// dbg!(new_utc_datetime);
		virtual_datetimes.push(new_utc_datetime);
	}
	Ok(virtual_datetimes)

}  // end of function


pub fn future_foo(cron_expression_string: &str, _cron_timezone: Tz, _number_of_results: usize) -> () {

	/* Concept
	
		1. Take the Local Timezone cron expression string.
		2. Create a Struct instance from that.
		3. Based on this Local Cron, create a Vector of all possible UTC Cron Expressions.  There could be half a dozen.
		4. Loop through each UTC Cron Expression, and create the next N scheduled UTC datetimes.
		5. We now have M sets of N datetimes.
		6. Merge them, and eliminate uniques.
		7. Return the last of UTC Datetimes to the caller.  These are the next N run times.
	*/

	match cron_str_to_cron_str7(cron_expression_string) {
		Ok(cron_string) => {

			// We now have a 7-element cron string.
			match Schedule::from_str(&cron_string) {
				Ok(_schedule) => {
					// Returns UTC Datetimes that are *after* the current UTC datetime now.
					// Unfortunately, UTC appears to be the only option.
					// return schedule.upcoming(Utc).take(10).next();
				},
				Err(error) => {
					println!("ERROR: Cannot parse invalid cron string: '{}'.  Error: {}", cron_string, error);
					// return None;
				}
			}
		},
		Err(error) => {
			println!("ERROR: Cannot parse invalid cron string: '{}'.  Error: {}", cron_expression_string, error);
			// return None;
		}
	}
	()
} // end function 'future_foo'


/*

use std::{convert::TryInto};

struct CronStruct {
	second: Option<String>,
	minute: Option<String>,
	hour: Option<String>,
	day_of_month: Option<String>,
	month: Option<String>,
	day_of_week: Option<String>,
	year: Option<String>
}

impl CronStruct {

	fn to_string(&self) -> String {

		let result = format!("{} {} {} {} {} {} {}",
			self.second.unwrap_or("*".to_owned()),
			self.minute.unwrap_or("*".to_owned()),
			self.hour.unwrap_or("*".to_owned()),
			self.day_of_month.unwrap_or("*".to_owned()),
			self.month.unwrap_or("*".to_owned()),
			self.day_of_week.unwrap_or("*".to_owned()),
			self.year.unwrap_or("*".to_owned())
		);
		result
	}
}

impl FromStr for CronStruct {
	type Err = CronError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {

		fn nonwildcard_or_none(element: &str) -> Option<String> {
			if element == "*" {
				return None
			}
			else {
				return Some(element.to_owned())
			}
		}

		let cron7_expression: String = cron_str_to_cron_str7(s)?;
		let vector_cron7: Vec<&str> = cron7_expression.split(" ").collect();

		Ok(CronStruct {
			second: nonwildcard_or_none(vector_cron7[0]),
			minute: nonwildcard_or_none(vector_cron7[1]),
			hour: nonwildcard_or_none(vector_cron7[2]),
			day_of_month: nonwildcard_or_none(vector_cron7[3]),
			month: nonwildcard_or_none(vector_cron7[4]),
			day_of_week: nonwildcard_or_none(vector_cron7[5]),
			year: nonwildcard_or_none(vector_cron7[6]),
		})
	}
}

pub fn cron_tz_to_cron_utc(cron_expression: &str, timezone: Tz) -> Result<Vec<String>, CronError> {
	/*
		Input: A timezone-specific Cron Expression.
		Output: A vector of UTC Cron Expression.

		Inspired and derived from: https://github.com/Sonic0/local-crontab ...
		... which itself was derived from https://github.com/capitalone/local-crontab created by United Income at Capital One.
	*/
	println!("Ok, will try to convert cron '{}' with time zone '{}' to a vector of UTC cron expressions.", cron_expression, timezone);

	let cron_struct: CronStruct = cron_expression.parse()?;

	// If the hour part of Cron is the entire range of hours (*), then not much to do.
	if cron_struct.hour.is_none() {
		return Ok(vec!(cron_struct.to_string()));
	}
	
	// Create the nested list with every single day belonging to the cron
	let utc_list_crontabs = _day_cron_list(cron_struct);
	// Group hours together
	utc_list_crontabs = _group_hours(utc_list_crontabs)
	// Group days together
	utc_list_crontabs = _group_days(utc_list_crontabs)
	// Convert a day-full month in *
	utc_list_crontabs = _range_to_full_month(utc_list_crontabs)
	// Group months together by hour / minute & days
	utc_list_crontabs = _group_months(utc_list_crontabs)

	let mut cron_strings: Vec<String> = Vec::new();
	for cron_list in utc_list_crontabs.iter() {
		let next_cron = CronStruct::from_integer_array(cron_list);
		let next_cron_string = next_cron.to_string();
		cron_strings.append(cron_str_to_cron_str7(next_cron_string));
	}
	Ok(cron_strings)
}

type CronConverterNestedLists = Vec<Vec<Vec<u32>>>;

fn _day_cron_list(cron_struct: CronStruct) -> CronConverterNestedLists {
	/* 
		Returns a nested list struct in which each element represents every single day in cron list format,
		readable by Cron-Converter Object. Sometimes days included in the cron range do not exist in the real life for every month(example: February 30),
		so these days will be discarded.
		:return: acc (list of ints): nested list made up of cron lists readable by Cron-Converter Object.
	*/

	/*
	let utc_list_crontabs = Vec::new();
	for month in cron_struct.month {
		for day in cron_struct.day {
			for hour in self.localized_cron_list[1]:
				try:
					local_date = datetime(self.cron_year, month, day, hour, 0, tzinfo=self.timezone)
				except ValueError:
					continue  # skip days that not exist (eg: 30 February)
				utc_date = (local_date - local_date.utcoffset()).replace(tzinfo=timezone.utc)
				# Create one Cron list for each hour
				utc_list_crontabs.append([
					[minute for minute in self.localized_cron_list[0]],
					[utc_date.hour],
					[utc_date.day], [utc_date.month], self.localized_cron_list[4]])
		}
	}
	utc_list_crontabs
	*/	
}

*/

/*

	crontab = Converter(valid_cron.get('in'), valid_cron.get('timezone'))
	result = crontab.to_utc_crons()

		self.localized_cron_list = self.localized_cron.to_list()
		if not timezone_str:
			self.timezone = tz.tzlocal()  # Use current Local Timezone if no input timezone
		elif tz.gettz(timezone_str):
			self.timezone = tz.gettz(timezone_str)
		else:
			raise WrongTimezoneError("Incorrect Timezone string")
		self.cron_year = year if bool(year) else datetime.now(tz=self.timezone).year

	def to_utc_cron(self) -> str:
		"""Function to convert a localized cron string to UTC cron string.
		This function converts only hour and day part, however the result in not reliable in some cases due to not handled DST.
		:return: cron_string (str): the resulting cron readable by all systems.
		"""
		# If the hour part of Cron is the entire range of hours (*) is useless proceed
		if self.localized_cron.parts[1].is_full():
			return self.localized_cron.to_string()
		# Get offset from utc in hours
		local_offset = self.timezone.utcoffset(datetime.now(self.timezone))
		local_offset_hours = int(local_offset.total_seconds() / 3600)  # offset in second / second in an hour

		utc_cron_list = self.localized_cron_list
		day_shift = (False, 0)
		hour_shifted_count = 0
		# Hours shift
		hour_range = self.localized_cron.parts[1].possible_values()  # Range of hours that a Cron hour object Part can assume
		cron_hours_part_utc = [hour - local_offset_hours for hour in self.localized_cron_list[1]]  # Shift hour based of offset from UTC
		for idx, hour in enumerate(cron_hours_part_utc):
			if hour < hour_range[0]:
				# Hour < 0 (ex: -2, -1) as intended in the previous day, so shift them to a real hour (ex: 22, 23)
				day_shift = (True, -1)
				hour += len(hour_range)  # Convert negative hour to real (ex: -2 + 24 = 22, -1 + 24 = 23)
				cron_hours_part_utc.pop(idx)
				cron_hours_part_utc.insert(idx, hour)
				hour_shifted_count += 1
			elif hour > hour_range[-1]:
				# Hour < 0 (ex: -2, -1) as intended in the previous day, so shift them to a real hour (ex: 22, 23)
				day_shift = (True, 1)
				hour -= len(hour_range)  # Convert not existing hour to real (ex: 25 - 24 = 1, 26 - 24 = 2)
				cron_hours_part_utc.pop(idx)
				cron_hours_part_utc.insert(idx, hour)
				hour_shifted_count += 1
		utc_cron_list[1] = cron_hours_part_utc

		# Day shift
		# if it is necessary a day shift and the original days Cron Part is not full(*)
		if day_shift[0] and not self.localized_cron.parts[2].is_full():
			# All hours shifted to the a next or previous day
			if day_shift[0] and hour_shifted_count == len(cron_hours_part_utc):
				utc_cron_list[2] = [day + day_shift[1] for day in self.localized_cron_list[2]]
			# Only one or more hours shifted to the a next or previous day
			elif day_shift[0] and hour_shifted_count != len(cron_hours_part_utc):
				raise ValueError("Operation cross days not supported. Sorry! (╥﹏╥)")

		utc_cron = Cron()
		utc_cron.from_list(utc_cron_list)

		return utc_cron.to_string()



	def _range_to_full_month(self, utc_list_crontabs: CronConverterNestedLists) -> CronConverterNestedLists:
		"""Returns a modified list with the character '*' as month in case of the month is day-full.
		The Cron-Converter read a full month only if it has 31 days.
		:return: acc (nested list of ints): modified nested list made up of cron lists readable by Cron-Converter Object.
		"""
		acc = []
		for element in utc_list_crontabs:
			if len(element[2]) == monthrange(self.cron_year, element[3][0])[1]:
				element[2] = [day for day in range(1, 32)]

			acc.append(element)
		return acc

	@staticmethod
	def _group_hours(utc_list_crontabs: CronConverterNestedLists) -> CronConverterNestedLists:
		"""Group hours together by minute, day and month.
		:param utc_list_crontabs: Nested list of crontabs not grouped.
		:return: acc (nested list of ints): filtered nested list made up of cron lists readable by Cron-Converter Object.
		"""
		acc = []
		for element in utc_list_crontabs:
			if len(acc) > 0 and \
					acc[-1][0] == element[0] and \
					acc[-1][2] == element[2] and \
					acc[-1][3] == element[3]:
				acc[-1][1].append(element[1][0])
			else:
				acc.append(element)
		return acc

	@staticmethod
	def _group_days(utc_list_crontabs: CronConverterNestedLists) -> CronConverterNestedLists:
		"""Group days together by hour, minute and month.
		:param utc_list_crontabs: Nested list of crontabs previously grouped in hours.
		:return: acc (nested list of ints): filtered nested list made up of cron lists readable by Cron-Converter Object.
		"""
		acc = []
		for element in utc_list_crontabs:
			if len(acc) > 0 and \
					acc[-1][0] == element[0] and \
					acc[-1][1] == element[1] and \
					acc[-1][3] == element[3]:
				acc[-1][2].append(element[2][0])
			else:
				acc.append(element)
		return acc

	@staticmethod
	def _group_months(utc_list_crontabs: CronConverterNestedLists) -> CronConverterNestedLists:
		"""Group months together by minute, days and hours
		:param utc_list_crontabs: Nested list of crontabs previously grouped in days.
		:return: acc (nested list of ints): filtered nested list made up of cron lists readable by Cron-Converter Object.
		"""
		acc = []
		for element in utc_list_crontabs:
			if len(acc) > 0 and \
					acc[-1][0] == element[0] and \
					acc[-1][1] == element[1] and \
					acc[-1][2] == element[2]:
				acc[-1][3].append(element[3][0])
			else:
				acc.append(element)
		return acc

*/