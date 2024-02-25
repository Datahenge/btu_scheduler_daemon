// email.rs

// https://github.com/lettre/lettre/discussions

use anyhow::{Context as AHContext, Result as AHResult};
use chrono::{SecondsFormat, Utc};
use lettre::{transport::smtp::authentication::Credentials, Message, SmtpTransport, Transport};
// use lettre::smtp::response::Response;
// use lettre_email::{Email, EmailBuilder};
use tracing::{trace, debug, info, warn, error, span, Level};
use crate::config::AppConfig;


#[derive(Clone, Debug)]
pub struct BTUEmail {
    from: String,
    to: Vec<String>,
    subject: String,
    body: String
}

pub fn send_email(app_config: &AppConfig, subject: &str, body: &str) -> AHResult<()> {
    
    let mailer = make_mailer_from_config(app_config)?;

    // Need to create a semi-colon separated string of To Email addresses.
    let mut to_addresses = String::new();
    for each in app_config.email_addresses.as_ref().unwrap() {
        to_addresses += &format!("{};", each);
    }

    // This seems very silly, looping over the entire set of functions.
    // But Lettre 0.10 seems a step backwards, and I don't have time to fix

	let btu_email = BTUEmail {
        from: app_config.email_address_from.as_ref().unwrap().to_owned(),
        to: app_config.email_addresses.as_ref().unwrap().to_vec(),
        subject: subject.to_owned(),
        body: body.to_owned()
    };

    // Add multiple To Address, if required.
    for each_recipient in btu_email.to {

        let this_body = body;
        // Create an Email Builder.
        let email: Message = Message::builder()
        .from(btu_email.from.parse().unwrap())  // parse the String into a Mailbox
        .to(each_recipient.parse().unwrap())
        .subject(&btu_email.subject)
        .body(this_body.to_owned())
        .unwrap();

        match mailer.send(&email) {
            Ok(_) => {
                println!("Email sent successfully!");
            }
            Err(e) => panic!("Could not send email: {:?}", e),
        }
    
    }

    Ok(())
}


pub fn make_mailer_from_config(app_config: &AppConfig) -> AHResult<SmtpTransport> {

    if app_config.email_host_name.is_none() {
        panic!("Configuration file is missing an Email Host Name.");
    }
    if app_config.email_account_name.is_none() {
        panic!("Configuration file is missing an Email Account Name.");
    }
    if app_config.email_account_password.is_none() {
        panic!("Configuration file is missing an Email Password.");
    }

    let this_email_account: String = app_config.email_account_name.as_ref().unwrap().clone();
    let this_email_password: String = app_config.email_account_password.as_ref().unwrap().clone();
    let this_email_host: String = app_config.email_account_password.as_ref().unwrap().clone();

    let creds = Credentials::new(this_email_account, this_email_password);

    // Open a remote connection to mail server.
    let mailer = SmtpTransport::relay(&this_email_host)
        .unwrap()
        .credentials(creds)
        .build();

    Ok(mailer)
}

/*
fn make_transport(mail_host: &str, mail_username: String, mail_password: String) -> AHResult<SmtpTransport> {
	let transport: SmtpTransport = SmtpClient::new_simple(&mail_host)
		.unwrap()
		.credentials(Credentials::new(mail_username, mail_password))
		.transport();
	Ok(transport)
}
*/


pub fn make_email_body_preamble(app_config: &AppConfig) -> String {
    
    let preamble: String = format!("{}<br>{}<br>{}<br>",
        "Hi, I am the BTU scheduler daemon.",
        format!("The current time is {} (UTC).", Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)),
        format!("My environment is named: {}", app_config.environment_name.as_ref().unwrap_or(&"Not Specified".to_owned()))
    );

    preamble
}