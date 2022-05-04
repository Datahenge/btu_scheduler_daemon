use lettre::{SmtpClient, SmtpTransport, Transport};
use lettre::smtp::authentication::Credentials;
use lettre::smtp::response::Response;
use lettre_email::EmailBuilder;


#[derive(Clone, Debug)]
struct MyEmail {
    from: String,
    to: String,
    subject: String,
    body: String
}

fn send_email(mut transport: SmtpTransport, email: MyEmail) -> Result<Response, lettre::smtp::error::Error> {
    
    // Transport requires mutable ownership; not sure why.
    // We need to take ownership of 'email', because EmailBuilder wants to own its arguments.
    let email = EmailBuilder::new()
        .from(email.from)
        .to(email.to)
        .subject(email.subject)
        .html(email.body)
        .build()
        .unwrap();

    return transport.send(email.into())
}

fn make_transport(mail_host: &str, mail_username: String, mail_password: String) -> SmtpTransport {
	let transport: SmtpTransport = SmtpClient::new_simple(&mail_host)
		.unwrap()
		.credentials(Credentials::new(mail_username, mail_password))
		.transport();

	transport
}

/*
    let email = MyEmail {
        from: matches.value_of("email_from").unwrap().to_string(),
        to: matches.value_of("email_to").unwrap().to_string(),
        subject: matches.value_of("subject").unwrap().to_string(),
        body: matches.value_of("body").unwrap().to_string()
    }; 

    send_email(transport, email.clone()).unwrap();  // don't lose ownership of the original.

*/
