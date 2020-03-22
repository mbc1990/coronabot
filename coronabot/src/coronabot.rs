use crate::USDAILY_URL;
use slack::{Event, RtmClient, Message};
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use std::sync::{Arc, RwLock};
use num_format::{Locale, ToFormattedString};
use chrono::{DateTime, Utc, FixedOffset, NaiveDate};

#[derive(Serialize, Deserialize, Debug)]
struct USDailyStats {
    date: Option<u32>,
    states: Option<u32>,
    positive: Option<u32>,
    negative: Option<u32>,
    posNeg: Option<u32>,
    pending: Option<u32>,
    hospitalized: Option<u32>,
    death: Option<u32>,
    total: Option<u32>
}

pub struct Coronabot {
    bot_id: String,
    us_daily: Arc<RwLock<Option<Vec<USDailyStats>>>>
}

impl Coronabot {
    pub fn new(bot_id: String) -> Coronabot {
        // TODO: Start worker thread to download statistics
        return Coronabot{bot_id: bot_id, us_daily: Arc::new(RwLock::new(None))};
    }

    fn format_latest(&self, data: &Vec<USDailyStats>) -> String {

        let mut total_positive = 0;
        let mut total_negative = 0;
        let mut total_hospitalized = 0;
        let mut total_tested = 0;
        let mut total_dead = 0;
        let mut date = NaiveDate::parse_from_str("20200301", "%Y%m%d").unwrap();

        let last_el = data.last();
        match last_el {
            Some(el) => {
                println!("El: {:?}", el);
                let datestring = el.date.unwrap_or(0).to_string();
                date = NaiveDate::parse_from_str(&datestring, "%Y%m%d").unwrap();
                total_positive = el.positive.unwrap_or(0);
                total_negative = el.negative.unwrap_or(0);
                total_hospitalized = el.hospitalized.unwrap_or(0);
                total_tested = el.posNeg.unwrap_or(0);
                total_dead = el.death.unwrap_or(0);
            },
            None => {}
        }

        return format!("
        U.S. Overall stats ({date})\n \
        Total positive: {total_positive}\n \
        Total negative: {total_negative}\n \
        Total tested: {total_tested}\n \
        Total hospitalized: {total_hospitalized}\n \
        Total deaths: {total_dead}",
                       date=date,
                       total_positive=total_positive.to_formatted_string(&Locale::en),
                       total_negative=total_negative.to_formatted_string(&Locale::en),
                       total_hospitalized=total_hospitalized.to_formatted_string(&Locale::en),
                       total_tested=total_tested.to_formatted_string(&Locale::en),
                       total_dead=total_dead.to_formatted_string(&Locale::en));
    }

    fn handle_mention(&self, text: String, channel: String, cli: &RtmClient) {
        let query_start = text.find(" ");
        match query_start {
            Some(q_string) => {
                let query = &text[q_string+1..text.len()];
                println!("Got query: {:?}", query);

                if query == "latest" {
                    let current_data = self.us_daily.read().unwrap();
                    match &*current_data {
                        Some(data) => {

                            // let to_send = serde_json::to_string(&data).unwrap();
                            let to_send = self.format_latest(data);
                            cli.sender().send_message(&channel, &to_send);
                        },
                        _ => {}
                    }
                } else if query == "help" {
                    let to_send = "Usage: @Coronabot <help|latest>";
                    cli.sender().send_message(&channel, &to_send);
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
                let parsed: Vec<USDailyStats> = serde_json::from_str(&body).unwrap();
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