/*
	DEV NOTE: To run tests and display STDOUT, type the following in the shell:
		cargo test -- --nocapture
*/

#[cfg(test)]
mod tests {

	use crate::btu_cron::cron_str_to_cron_str7;
	use crate::btu_cron::{local_cron_to_utc_datetimes};
	use chrono::Utc;
	use chrono_tz::Tz;

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

	#[test]
	fn test_cron7_success() {
		// Format for cron7:	<seconds> <minutes> <hours> <day-of-month> <month> <day-of-week> <year>

		let expression_five = "30,45 14 ? 1-5 Monday"; 		// At 2:30 p.m. and 2:45 p.m. every Monday in the months January to May (1-5)
		let expression_six = "30,45 14 ? 1-5 Monday 2021"; // At 2:30 p.m. and 2:45 p.m. every Monday in the months January to May (1-5), in year 2021
		let expression_seven = "25 30 10 * * ? 2021";  		// At 10:30:25 a.m. every day in the year 2021

		assert_eq!(
			cron_str_to_cron_str7(expression_five).unwrap(),
			"* 30,45 14 ? 1-5 Monday *"
        );

		assert_eq!(
			cron_str_to_cron_str7(expression_six).unwrap(),
			"* 30,45 14 ? 1-5 Monday 2021"
        );

        assert_eq!(
			cron_str_to_cron_str7(expression_seven).unwrap(),
			expression_seven
        );
    }
	
	#[test]
	fn test_simple_local_cron() {

		let number_of_results: usize = 2;
		let local_timezone = chrono_tz::America::Los_Angeles;

		println!("TEST: UTC Now is {}", Utc::now());
		println!("TEST: Trying to retrieve {} results from function.", number_of_results);

		let utc_expected = chrono::Utc::now();
		let vector_of_actual = local_cron_to_utc_datetimes("0 30 3 * * * 2021", local_timezone, number_of_results).unwrap();
		if vector_of_actual.len() < 1 {
			panic!("No values were returned from function 'local_cron_to_utc_datetimes'");
		}
		assert_eq!(utc_expected, vector_of_actual[0]);  // compare first value.
	}

	/* Function Not-Yet-Implemented

	use crate::cron::future_foo;

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
