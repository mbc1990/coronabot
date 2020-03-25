use crate::{USDAILY_URL, STATESDAILY_URL};
use slack::{Event, RtmClient, Message};
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use std::sync::{Arc, RwLock};
use num_format::{Locale, ToFormattedString};
use chrono::{DateTime, Utc, FixedOffset, NaiveDate};
use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct DailyStats {
    state: Option<String>,
    date: Option<u32>,
    positive: Option<u32>,
    negative: Option<u32>,
    pending: Option<i32>,
    hospitalized: Option<u32>,
    death: Option<u32>,
    total: Option<u32>,
}

pub struct Coronabot {
    bot_id: String,
    us_daily: Arc<RwLock<Option<Vec<DailyStats>>>>,
    states_daily: Arc<RwLock<Option<HashMap<String, Vec<DailyStats>>>>>
}

fn construct_states_map(data: &Vec<DailyStats>) -> HashMap<String, Vec<DailyStats>> {
    let mut ret = HashMap::new();
    for dp in data.iter() {
        let state = dp.state.clone().unwrap_or("N/A".to_string());
        match ret.entry(state) {
            Entry::Vacant(e) => {
                e.insert(vec![dp.clone()]);
            },
            Entry::Occupied(mut e) => {
                e.get_mut().push(dp.clone());
            }
        }
    }
    return ret;
}


impl Coronabot {
    pub fn new(bot_id: String) -> Coronabot {
        return Coronabot{bot_id: bot_id, us_daily: Arc::new(RwLock::new(None)), states_daily: Arc::new(RwLock::new(None))};
    }

    fn format_high_scores(&self, data: &HashMap<String, Vec<DailyStats>>) -> String {
        let mut pos_growth = 0;
        let mut pos_growth_state = "".to_string();

        let mut death_growth = 0;
        let mut death_growth_state = "".to_string();

        let mut mortality_rate = 0.0;
        let mut mortality_rate_state = "".to_string();


        let mut date = NaiveDate::parse_from_str("20200301", "%Y%m%d").unwrap();

        for (state, state_data) in data.iter() {
            let first_el = state_data.first();
            let mut today_pos = 0.0;
            let mut today_death = 0.0;
            let mut today_mortality = 0.0;
            let mut yesterday_pos = 0.0;
            let mut yesterday_death = 0.0;
            match first_el {
                Some(el) => {
                    let datestring = el.date.unwrap_or(0).to_string();
                    date = NaiveDate::parse_from_str(&datestring, "%Y%m%d").unwrap();

                    today_pos = el.positive.unwrap_or(0) as f64;
                    today_death = el.death.unwrap_or(0) as f64;
                    today_mortality = (today_death / today_pos) * 100.0;
                },
                None => {}
            }

            let yesterday= state_data.get(1);
            match yesterday {
                Some(el) => {
                    yesterday_pos = el.positive.unwrap_or(0) as f64;
                    yesterday_death = el.death.unwrap_or(0) as f64;
                },
                None => {}
            }

            let pos_growth_rate = (((today_pos - yesterday_pos) / yesterday_pos) * 100.0) as i32;
            if pos_growth_rate > pos_growth {
                pos_growth = pos_growth_rate;
                let el = first_el.unwrap();
                let state = el.state.clone().unwrap();
                pos_growth_state = state;

            }
            let death_growth_rate = (((today_death - yesterday_death ) / yesterday_death) * 100.0) as i32;
            if death_growth_rate > death_growth {
                death_growth = death_growth_rate;
                let el = first_el.unwrap();
                let state = el.state.clone().unwrap();
                death_growth_state = state;
            }

            if today_mortality > mortality_rate {
                mortality_rate = today_mortality;
                let el = first_el.unwrap();
                let state = el.state.clone().unwrap();
                mortality_rate_state = state;
                println!("{:} {:}", mortality_rate_state, mortality_rate);
            }
        }

        return format!("
        Daily High Scores ({date})\n \
        Mortality rate: {mortality_rate_state} ({mortality_rate:.2}%)\n \
        Positive tests growth: {pos_growth_state} (+{pos_growth}%)\n \
        Deaths growth: {death_growth_state} (+{death_growth}%)",
                       date=date,
                       mortality_rate=mortality_rate,
                       mortality_rate_state=mortality_rate_state,
                       pos_growth=pos_growth,
                       pos_growth_state=pos_growth_state,
                       death_growth=death_growth,
                       death_growth_state = death_growth_state);

    }

    fn format_daily(&self, data: &Vec<DailyStats>, geo_title: &str) -> String {
        let mut total_positive = 0;
        let mut total_negative = 0;
        let mut total_hospitalized = 0;
        let mut total_tested = 0;
        let mut total_dead = 0;
        let mut date = NaiveDate::parse_from_str("20200301", "%Y%m%d").unwrap();
        let mut death_rate = 0.0;

        let first_el = data.first();
        match first_el {
            Some(el) => {
                let datestring = el.date.unwrap_or(0).to_string();
                date = NaiveDate::parse_from_str(&datestring, "%Y%m%d").unwrap();
                total_positive = el.positive.unwrap_or(0);
                total_negative = el.negative.unwrap_or(0);
                total_hospitalized = el.hospitalized.unwrap_or(0);
                total_tested = total_positive + total_negative;
                total_dead = el.death.unwrap_or(0);
                death_rate = ((total_dead as f64 / total_positive as f64) * 100.0);
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
                // TODO: The casting in here fucks everything up, fix that
                let yesterday_total_tested = el.positive.unwrap_or(0) + el.negative.unwrap_or(0);
                pos_change = (((total_positive - el.positive.unwrap_or(0)) as f64 / el.positive.unwrap_or(0) as f64) * 100.0) as i32;
                neg_change = (((total_negative - el.negative.unwrap_or(0)) as f64 / el.negative.unwrap_or(0) as f64) * 100.0) as i32;
                hosp_change = (((total_hospitalized - el.hospitalized.unwrap_or(0)) as f64 / el.hospitalized.unwrap_or(0) as f64) * 100.0) as i32;
                tested_change = (((total_tested - yesterday_total_tested) as f64 / yesterday_total_tested as f64) * 100.0) as i32;
                dead_change = (((total_dead - el.death.unwrap_or(0)) as f64 / el.death.unwrap_or(0) as f64) * 100.0) as i32;
            },
            None => {}
        }

        return format!("
        {geo_title} Overall Daily Stats ({date})\n \
        Total positive: {total_positive} (+{pos_change}%)\n \
        Total negative: {total_negative} (+{neg_change}%)\n \
        Total tested: {total_tested} (+{tested_change}%)\n \
        Total hospitalized: {total_hospitalized} (+{hosp_change}%)\n \
        Mortality rate: {death_rate:.2}% \n \
        Souls lost: {total_dead} (+{dead_change}%)",
                       geo_title=geo_title,
                       date=date,
                       death_rate=death_rate,
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
                            // let to_send = self.format_latest(data);
                            let to_send = self.format_daily(data, "U.S.");
                            println!("Sending data");
                            cli.sender().send_message(&channel, &to_send);
                        },
                        None => {
                            let to_send = "Sorry, country-level data is missing. Is the API working?";
                        }
                    }
                } else if query == "help" {
                    let to_send = "Usage: @Coronabot <help|latest|<2 letter state abbreviation>|high scores>";
                    cli.sender().send_message(&channel, &to_send);
                } else if query == "high scores" {
                    let state_stats = self.states_daily.read().unwrap();
                    match &*state_stats {
                        Some(data) => {
                            let to_send = self.format_high_scores(data);
                            cli.sender().send_message(&channel, &to_send);
                        },
                        None => {
                            let to_send = "Sorry, state-level data is missing. Is the API working?";
                            cli.sender().send_message(&channel, &to_send);
                        }
                    }

                } else {
                    let state_stats = self.states_daily.read().unwrap();
                    match &*state_stats {
                        Some(data) => {
                            if !data.contains_key(query) {
                                let to_send = format!("State data is present but does not contain stats for {state}", state=query);
                                cli.sender().send_message(&channel, &to_send);
                                return;
                            }
                            let state_data = data.get(query).unwrap();
                            let to_send = self.format_daily(state_data, &query);
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

    pub fn start_bg_update(&self) {
        let my_us_daily = self.us_daily.clone();
        let my_states_daily = self.states_daily.clone();
        thread::spawn(move || {
            loop {
                println!("Making US daily query...");
                let body = reqwest::blocking::get(USDAILY_URL)
                    .unwrap()
                    .text()
                    .unwrap();
                let parsed: Vec<DailyStats> = serde_json::from_str(&body).unwrap();
                let mut data = my_us_daily
                    .write()
                    .unwrap();
                *data = Some(parsed);

                // We have to manually drop this to release the RwLock since we sleep in the same closure
                drop(data);

                println!("Making states daily query...");
                let body = reqwest::blocking::get(STATESDAILY_URL)
                    .unwrap()
                    .text()
                    .unwrap();
                let parsed: Vec<DailyStats> = serde_json::from_str(&body).unwrap();
                let states_map = construct_states_map(&parsed);
                let mut states_data = my_states_daily
                    .write()
                    .unwrap();
                *states_data = Some(states_map);
                drop(states_data);

                // Rerun once an hour
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