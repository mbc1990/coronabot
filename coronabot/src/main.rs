mod coronabot;
extern crate slack;

use slack::RtmClient;
use crate::coronabot::Coronabot;


fn main() {
    println!("Starting Coronabot");
    let args: Vec<String> = std::env::args().collect();
    let api_key = args[1].clone();
    let bot_id = args[2].clone();
    println!("API key: {:?}", api_key);
    println!("Bot id: {:?}", bot_id);
    let mut handler = Coronabot::new(bot_id);
    let r = RtmClient::login_and_run(&api_key, &mut handler);
    match r {
        Ok(_) => {}
        Err(err) => println!("Error: {}", err),
    }
}
