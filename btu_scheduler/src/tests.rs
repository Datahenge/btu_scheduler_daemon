/*
	NOTE: To run tests and display STDOUT, type the following in the shell:
	
		cargo test -- --nocapture

*/

#[cfg(test)]
mod tests {
	
	use chrono::{DateTime, NaiveDateTime, Utc};
	use crate::btu_cron::cron_str_to_cron_str7;
	use crate::btu_cron::{tz_cron_to_utc_datetimes};
	use crate::scheduler::RQScheduledTask;

    #[test]
    fn test_cron7_fail() {
		// Format for cron7:	<seconds> <minutes> <hours> <day-of-month> <month> <day-of-week> <year>
        let expression_four = "1 2 3 4";
		let expression_eight = "1 2 3 4 5 6 7 8";

		// Dev Note: Accomplishing the below required implementing trait 'PartialEq' for the enum CronError.
		let failed_test = cron_str_to_cron_str7(expression_four);
        assert!(failed_test.is_err());
		assert_eq!(failed_test.err().unwrap(), crate::error::CronError::WrongQtyOfElements { found: 4 });

		let failed_test = cron_str_to_cron_str7(expression_eight);
        assert!(failed_test.is_err());
		assert_eq!(failed_test.err().unwrap(), crate::error::CronError::WrongQtyOfElements { found: 8 });
    }


	/**
	 * This test is to ensure that I can convert a 5, 6, or 7 character cron string, to a 7-character cron string.
	 */
	#[test]
	fn test_cron7_success() {
		// Format for cron7:	<seconds> <minutes> <hours> <day-of-month> <month> <day-of-week> <year>

		let expression_five = "30,45 14 ? 1-5 Monday"; 		// At 2:30 p.m. and 2:45 p.m. every Monday in the months January to May (1-5)
		let expression_six = "30,45 14 ? 1-5 Monday 2021"; // At 2:30 p.m. and 2:45 p.m. every Monday in the months January to May (1-5), in year 2021
		let expression_seven = "25 30 10 * * ? 2021";  		// At 10:30:25 a.m. every day in the year 2021

		assert_eq!(
			cron_str_to_cron_str7(expression_five).unwrap(),
			"0 30,45 14 ? 1-5 Monday *"
        );

		assert_eq!(
			cron_str_to_cron_str7(expression_six).unwrap(),
			"0 30,45 14 ? 1-5 Monday 2021"
        );

        assert_eq!(
			cron_str_to_cron_str7(expression_seven).unwrap(),
			expression_seven
        );
    }

	/**
	 * This test proves that a Local Cron is corrected converted to a UTC Datetime.
	 */	
	// #[test]
	fn test_simple_local_cron() {
		use chrono::TimeZone;

		let local_timezone = chrono_tz::America::Los_Angeles;
		let starting_at_utc_datetime: DateTime<Utc> = Utc.ymd(2021, 12, 25).and_hms(0, 0, 1);

		let number_of_results: usize = 3;  // We want the first 3 results back.

		// Every 10 minutes starting at 1am on December 25th, 2021.
		let vec_utc_calculated = tz_cron_to_utc_datetimes("0 */10 1 25 12 * 2021", 
		                                                  local_timezone,
														  Some(starting_at_utc_datetime),
														  number_of_results).unwrap();

		// There is an 8-hour difference between Los Angeles and UTC in December.
		// Therefore, with the cron string above, the expected results begin at 9AM UTC.
		let vec_utc_expected = vec![
			Utc.ymd(2021, 12, 25).and_hms(9, 0, 0),		// `2021-12-25T09:00:00Z`
			Utc.ymd(2021, 12, 25).and_hms(9, 10, 0),    // `2021-12-25T09:10:00Z`
			Utc.ymd(2021, 12, 25).and_hms(9, 20, 0)     // `2021-12-25T09:20:00Z`
		];
		assert_eq!(vec_utc_expected, vec_utc_calculated);
	}


	#[test]
	fn test_2_simple_local_cron() {
		use chrono::TimeZone;

		let local_timezone = chrono_tz::America::Los_Angeles;
		let starting_at_utc_datetime: DateTime<Utc> = Utc.ymd(2021, 12, 25).and_hms(0, 0, 1);
		let number_of_results: usize = 3;  // We want the first 3 results back.

		// Every 30 minutes starting at 12:00:01 am on December 25th, 2021.
		let vec_utc_calculated = tz_cron_to_utc_datetimes("*/30 * * * *", 
		                                                  local_timezone,
														  Some(starting_at_utc_datetime),
														  number_of_results).unwrap();

		// There is an 8-hour difference between Los Angeles and UTC in December.
		// Therefore, with the cron string above, the expected results begin at 9AM UTC.
		let vec_utc_expected = vec![
			Utc.ymd(2021, 12, 25).and_hms(0, 30, 0),  // `2021-12-25T00:30:00Z`
			Utc.ymd(2021, 12, 25).and_hms(1, 0, 0),   // `2021-12-25T01:00:00Z`
			Utc.ymd(2021, 12, 25).and_hms(1, 30, 0)   // `2021-12-25T01:30:00Z`
		];
		assert_eq!(vec_utc_expected, vec_utc_calculated);
	}


	/**
	 * This test demonstrates how we can coerce a Tuple of 2 Strings into an RQ Scheduled Task.
	 */
	#[test]
	fn test_rqscheduledtask_from_strings() {
		
		let job_id = "Job12345".to_string();
		let unix_timestamp: i64 = 1638424800;
		let datetime_naive = NaiveDateTime::from_timestamp(unix_timestamp, 0);
		let datetime_utc: DateTime<Utc> = DateTime::from_utc(datetime_naive, Utc);

		// Create a new struct: RQScheduledTask
		let expected = RQScheduledTask {
			task_schedule_id: job_id.clone(),
			next_datetime_unix: unix_timestamp,
			next_datetime_utc: datetime_utc,
		};

		// Create from a Tuple of 2 Strings:
		let actual = RQScheduledTask::from(
			(job_id, unix_timestamp.to_string())
		);
		assert_eq!(expected, actual);
	}

	/* Feature below is Not-Yet-Implemented.

	use crate::cron::future_foo;
	use chrono_tz::Tz;

	#[test]
	fn test_cron_to_utc_cron() {
		// Format for cron7:	<seconds> <minutes> <hours> <day-of-month> <month> <day-of-week> <year>

		let expected_result: Vec<String> = vec!(
			"0 15 * 1-2,12 *".to_string(),
			"0 15 1-10 3 *".to_string(),
			"0 14 11-31 3 *".to_string(),
			"0 14 * 4-10 *".to_string(),
			"0 14 1-3 11 *".to_string(),
			"0 15 4-31 11 *".to_string()
		);

		let timezone: Tz = "America/New_York".parse().unwrap();
		assert_eq!(
			future_foo("0 10 * * *", timezone, 6).unwrap(),
			expected_result
        );
	}
 	*/
}
