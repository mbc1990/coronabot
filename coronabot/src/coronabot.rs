use crate::USDAILY_URL;
use slack::{Event, RtmClient, Message};
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use std::sync::{Arc, RwLock};
use num_format::{Locale, ToFormattedString};
use chrono::{DateTime, Utc, FixedOffset, NaiveDate};
use std::collections::HashMap;

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

/*
{"date":20200323,"state":"WA","positive":1996,"negative":28879,"pending":null,"hospitalized":null,"death":95,"total":30875,"dateChecked":"2020-03-23T20:00:00Z"}
*/
#[derive(Serialize, Deserialize, Debug)]
struct StateDailyStats {
    date: Option<u32>,
    state: Option<String>,
    positive: Option<u32>,
    negative: Option<u32>,
    pending: Option<u32>,
    hospitalized: Option<u32>,
    death: Option<u32>,
    total: Option<u32>,
    dateChecked: Option<String>
}

pub struct Coronabot {
    bot_id: String,
    us_daily: Arc<RwLock<Option<Vec<USDailyStats>>>>,
    states_daily: Arc<RwLock<Option<HashMap<String, Vec<StateDailyStats>>>>>
}

impl Coronabot {
    pub fn new(bot_id: String) -> Coronabot {
        // TODO: Start worker thread to download statistics
        return Coronabot{bot_id: bot_id, us_daily: Arc::new(RwLock::new(None)), states_daily: Arc::new(RwLock::new(None))};
    }

    fn format_state_latest(&self, data: &HashMap<String,Vec<StateDailyStats>>, state: &str) -> String {
        let mut total_positive = 0;
        let mut total_negative = 0;
        let mut total_hospitalized = 0;
        let mut total_tested = 0;
        let mut total_dead = 0;
        let mut date = NaiveDate::parse_from_str("20200301", "%Y%m%d").unwrap();
        let mut pos_change = 0;
        let mut neg_change = 0;
        let mut hosp_change = 0;
        let mut tested_change = 0;
        let mut dead_change :i32 = 0;

        return format!("
        {state} Daily Stats ({date})\n \
        Total positive: {total_positive} (+{pos_change}%)\n \
        Total negative: {total_negative} (+{neg_change}%)\n \
        Total tested: {total_tested} (+{tested_change}%)\n \
        Total hospitalized: {total_hospitalized} (+{hosp_change}%)\n \
        Souls lost: {total_dead} (+{dead_change}%)",
                       state=state,
                       date=date,
                       total_positive=total_positive.to_formatted_string(&Locale::en),
                       pos_change=pos_change,
                       total_negative=total_negative.to_formatted_string(&Locale::en),
                       neg_change=neg_change,
                       total_hospitalized=total_hospitalized.to_formatted_string(&Locale::en),
                       hosp_change=hosp_change,
                       total_tested=total_tested.to_formatted_string(&Locale::en),
                       tested_change=tested_change,
                       total_dead=total_dead.to_formatted_string(&Locale::en),
                       dead_change=dead_change);

    }

    fn format_latest(&self, data: &Vec<USDailyStats>) -> String {

        let mut total_positive = 0;
        let mut total_negative = 0;
        let mut total_hospitalized = 0;
        let mut total_tested = 0;
        let mut total_dead = 0;
        let mut date = NaiveDate::parse_from_str("20200301", "%Y%m%d").unwrap();

        let first_el = data.first();
        match first_el {
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

        let mut pos_change = 0;
        let mut neg_change = 0;
        let mut hosp_change = 0;
        let mut tested_change = 0;
        let mut dead_change :i32 = 0;

        let yesterday_el = data.get(1);
        match yesterday_el {
            Some(el) => {
                pos_change = (((total_positive - el.positive.unwrap_or(0)) as f64 / el.positive.unwrap_or(0) as f64) * 100.0) as i32;
                neg_change = (((total_negative - el.negative.unwrap_or(0)) as f64 / el.negative.unwrap_or(0) as f64) * 100.0) as i32;
                hosp_change = (((total_hospitalized- el.hospitalized.unwrap_or(0)) as f64 / el.hospitalized.unwrap_or(0) as f64) * 100.0) as i32;
                tested_change = (((total_tested - el.posNeg.unwrap_or(0)) as f64 / el.posNeg.unwrap_or(0) as f64) * 100.0) as i32;
                dead_change = (((total_dead - el.death.unwrap_or(0)) as f64 / el.death.unwrap_or(0) as f64) * 100.0) as i32;
            },
            None => {}
        }

        return format!("
        U.S. Overall Daily Stats ({date})\n \
        Total positive: {total_positive} (+{pos_change}%)\n \
        Total negative: {total_negative} (+{neg_change}%)\n \
        Total tested: {total_tested} (+{tested_change}%)\n \
        Total hospitalized: {total_hospitalized} (+{hosp_change}%)\n \
        Souls lost: {total_dead} (+{dead_change}%)",
                       date=date,
                       total_positive=total_positive.to_formatted_string(&Locale::en),
                       pos_change=pos_change,
                       total_negative=total_negative.to_formatted_string(&Locale::en),
                       neg_change=neg_change,
                       total_hospitalized=total_hospitalized.to_formatted_string(&Locale::en),
                       hosp_change=hosp_change,
                       total_tested=total_tested.to_formatted_string(&Locale::en),
                       tested_change=tested_change,
                       total_dead=total_dead.to_formatted_string(&Locale::en),
                       dead_change=dead_change);
    }

    fn handle_mention(&self, text: String, channel: String, cli: &RtmClient) {
        let query_start = text.find(" ");
        match query_start {
            Some(q_string) => {
                let query = &text[q_string+1..text.len()];
                println!("Got query: {:?}", query);


                if query == "latest" {
                    println!("Getting current data");
                    let current_data = self.us_daily.read().unwrap();
                    println!("Got data");

                    // Need to deref the rwlockguard, then borrow the option
                    // TODO: pretty ugly, is there a nicer way to do this?
                    match &*current_data {
                        Some(data) => {
                            println!("Getting data");
                            let to_send = self.format_latest(data);
                            println!("Sending data");
                            cli.sender().send_message(&channel, &to_send);
                        },
                        None => {
                            let to_send = "Sorry, country-level data is missing. Is the API working?";
                        }
                    }
                } else if query == "help" {
                    let to_send = "Usage: @Coronabot <help|latest>";
                    cli.sender().send_message(&channel, &to_send);
                } else {
                    let state_stats = self.states_daily.read().unwrap();
                    match &*state_stats {
                        Some(data) => {
                            let to_send = self.format_state_latest(data, &query);
                            cli.sender().send_message(&channel, &to_send);
                        },
                        None => {
                            let to_send = "Sorry, state-level data is missing. Is the API working?";
                            cli.sender().send_message(&channel, &to_send);
                        }
                    }
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
                println!("Making data query...");
                let body = reqwest::blocking::get(USDAILY_URL)
                    .unwrap()
                    .text()
                    .unwrap();
                println!("...Done");
                let parsed: Vec<USDailyStats> = serde_json::from_str(&body).unwrap();
                let mut data = my_us_daily.write().unwrap();
                *data = Some(parsed);
                drop(data);
                thread::sleep(Duration::from_millis(1000 * 60 * 60));
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