// email.rs

use anyhow::{Context as AHContext, Result as AHResult};
use lettre::{SmtpClient, SmtpTransport, Transport};
use lettre::smtp::authentication::Credentials;
use lettre::smtp::response::Response;
use lettre_email::{Email, EmailBuilder};
use tracing::{trace, debug, info, warn, error, span, Level};
use crate::config::AppConfig;


#[derive(Clone, Debug)]
pub struct BTUEmail {
    from: String,
    to: Vec<String>,
    subject: String,
    body: String
}

pub fn send_email(app_config: &AppConfig, subject: &str, body: &str) -> AHResult<lettre::smtp::response::Response> {
    
    let mut transport = make_transport_from_config(app_config)?;
    // Transport requires mutable ownership; not sure why.
    // We need to take ownership of 'email', because EmailBuilder wants to own its arguments.

    // Need to create a semi-colon separated string of To Email addresses.
    let mut to_addresses = String::new();
    for each in app_config.email_addresses.as_ref().unwrap() {
        to_addresses += &format!("{};", each);
    }

	let btu_email = BTUEmail {
        from: app_config.email_address_from.as_ref().unwrap().to_owned(),
        to: app_config.email_addresses.as_ref().unwrap().to_vec(),
        subject: subject.to_owned(),
        body: body.to_owned()
    };

    // Create an Email Builder.
    let mut email_builder = EmailBuilder::new()
        .from(btu_email.from)
        .subject(btu_email.subject)
        .html(btu_email.body);

    // Add multiple To Address, if required.
    for each in btu_email.to {
        email_builder = email_builder.to(each);
    }
    
    let email: Email = email_builder.build()?;
    let response = transport.send(email.into())?;
    Ok(response)
}


pub fn make_transport_from_config(app_config: &AppConfig) -> AHResult<SmtpTransport> {

    if app_config.email_host_name.is_none() {
        panic!("Configuration file is missing an Email Host Name.");
    }
    if app_config.email_account_name.is_none() {
        panic!("Configuration file is missing an Email Account Name.");
    }
    if app_config.email_account_password.is_none() {
        panic!("Configuration file is missing an Email Password.");
    }

    return make_transport(
        app_config.email_host_name.as_ref().unwrap(),
        app_config.email_account_name.as_ref().unwrap().to_owned(),
        app_config.email_account_password.as_ref().unwrap().to_owned()
    );
}

fn make_transport(mail_host: &str, mail_username: String, mail_password: String) -> AHResult<SmtpTransport> {
	let transport: SmtpTransport = SmtpClient::new_simple(&mail_host)
		.unwrap()
		.credentials(Credentials::new(mail_username, mail_password))
		.transport();
	Ok(transport)
}
