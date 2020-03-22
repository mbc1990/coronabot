use crate::USDAILY_URL;
use slack::{Event, RtmClient, Message};
use serde_json::Value;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, RwLock};

pub struct Coronabot {
    bot_id: String,
    us_daily: Arc<RwLock<Option<Value>>>
}

impl Coronabot {
    pub fn new(bot_id: String) -> Coronabot {
        // TODO: Start worker thread to download statistics
        return Coronabot{bot_id: bot_id, us_daily: Arc::new(RwLock::new(None))};
    }

    fn handle_mention(&self, text: String, channel: String, cli: &RtmClient) {
        let query_start = text.find(" ");
        match query_start {
            Some(q_string) => {
                let query = &text[q_string+1..text.len()];
                println!("Got query: {:?}", query);
                // TODO: Design UI/respond to queries

                let current_data = self.us_daily.read().unwrap();
                match &*current_data {
                    Some(data) => {
                        let to_send = serde_json::to_string(&data).unwrap();
                        cli.sender().send_message(&channel, &to_send);
                    },
                    _ => {}
                }
            },
            None => {
            }
        }
    }

    pub fn update_data(&mut self) {
        let my_us_daily = self.us_daily.clone();
        thread::spawn(move || {
            loop {
                let body = reqwest::blocking::get(USDAILY_URL)
                    .unwrap()
                    .text()
                    .unwrap();
                let parsed: Value = serde_json::from_str(&body).unwrap();
                let mut data = my_us_daily.write().unwrap();
                *data = Some(parsed);
                // println!("my us daily: {:?}", my_us_daily);

                // Check for updates once an hour
                // thread::sleep(Duration::from_millis(1000 * 60 * 60));
                thread::sleep(Duration::from_millis(5000 ));
            }
        });
    }
}

#[allow(unused_variables)]
impl slack::EventHandler for Coronabot {
    fn on_event(&mut self, cli: &RtmClient, event: Event) {
        match event {
            Event::Message(msg) => {
                match *msg {
                    Message::Standard(msg) => {
                        println!("msg: {:?}", msg);
                        let text = msg.text.unwrap();
                        println!("text: {:?}", text);
                        if text.contains(&self.bot_id) {
                            println!("Mentioned");
                            let channel = msg.channel.unwrap();
                            self.handle_mention(text, channel, cli);
                        }
                    },
                    _ => {}
                }
            },
            _ => {}
        }
    }
    fn on_close(&mut self, cli: &RtmClient) {
        println!("Connection closed");
    }

    fn on_connect(&mut self, cli: &RtmClient) {
        println!("Coronabot connected");
    }
}