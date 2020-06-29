use crate::{USDAILY_URL, STATESDAILY_URL};
use slack::{Event, RtmClient, Message};
use serde_json::Value;
use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use std::sync::{Arc, RwLock};
use num_format::{Locale, ToFormattedString};
use chrono::{DateTime, Utc, FixedOffset, NaiveDate, NaiveTime};
use gnuplot::{Figure, Caption, Color, AxesCommon, DashType};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use gnuplot::AutoOption::{Fix, Auto};
use gnuplot::TickOption::{Mirror, Format};
use gnuplot::LabelOption::{Font, TextColor};
use uuid::Uuid;
use s3::bucket::Bucket;
use s3::credentials::Credentials;
use std::fs::File;
use std::io::Read;
use gnuplot::PlotOption::{Axes, LineStyle, PointSize};
use gnuplot::XAxis::X1;
use gnuplot::YAxis::{Y1, Y2};


#[derive(Serialize, Deserialize, Debug, Clone)]
struct DailyStats {
    state: Option<String>,
    date: Option<u32>,
    positive: Option<u32>,
    negative: Option<u32>,
    pending: Option<u32>,
    hospitalized: Option<u32>,
    death: Option<u32>,
    total: Option<u32>,
}

pub struct Coronabot {
    bot_id: String,
    us_daily: Arc<RwLock<Option<Vec<DailyStats>>>>,
    states_daily: Arc<RwLock<Option<HashMap<String, Vec<DailyStats>>>>>

    // TODO: Should have a list of data sources that can be accessed
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
        Daily Worsts ({date})\n \
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

    fn generate_chart(&self,
                      x: Vec<i64>,
                      y1: Vec<f32>,
                      y2: Vec<f32>,
                      title: &str,
                      x1_name: &str,
                      y1_name: &str,
                      y2_name: &str) -> String {
        let mut fg = Figure::new();
        fg.axes2d()
            .set_title(&title, &[])
            .lines_points(
                &x,
                &y1,
                &[Axes(X1, Y1), Color("black"), PointSize(0.0)],
            )
            /*
            .lines_points(
                &x,
                &y2,
                &[Axes(X1, Y2), Color("blue"), PointSize(0.0)],
            )
            */
            .set_y_ticks(Some((Auto, 0)), &[Mirror(false)], &[])  // Make Y1 not mirror.
            // .set_y2_ticks(Some((Auto, 0)), &[Mirror(false), Format("%.2f")], &[])  // Make Y2 not mirror, and visible.
            .set_y_label(y1_name, &[TextColor("black")])
            // .set_y2_label(y2_name, &[TextColor("blue")])
            .set_x_ticks(Some((Auto, 1)), &[Mirror(false), Format("%m/%d")], &[Font("Helvetica", 12.0)])
            .set_x_time(true);


        println!("Saving to disk...");
        let mut fpath = "/tmp/".to_string();
        let uuid = Uuid::new_v4().to_string();
        fpath.push_str(&uuid);
        fpath.push_str(".png");
        let mut s3_path = "coronavirus/".to_string();
        s3_path.push_str(&uuid);
        s3_path.push_str(".png");
        let res = fg.save_to_png(&fpath.to_string(), 800, 400);
        match res {
            Ok(()) => {
                println!("Saved {:}", fpath);
                let credentials = Credentials::default();
                let region = s3::region::Region::UsEast1;
                let bucket = Bucket::new("image-paster", region, credentials).unwrap();
                let mut f = File::open(&fpath).unwrap();
                let mut buffer = Vec::new();
                f.read_to_end(&mut buffer).unwrap();
                bucket.put_object_blocking(&s3_path, &buffer, "multipart/form-data");
                let mut public_url = "https://image-paster.s3.amazonaws.com/".to_string();
                public_url.push_str(&s3_path);
                println!("Stored in s3: {:}", &public_url);
                return public_url;
            },
            Err(err) => {
                return format!("Sorry, there was an error generating your plot:\n {:?}", err);
            }
        }
    }

    // The total count data from the API *should* be monotonically increasing with time but
    // sometimes it isn't. This is a lousy hack so I don't have to deal with it for a little while
    fn safe_diff(&self, today: Option<u32>, yesterday: Option<u32>) -> u32 {
        let td= today.unwrap_or(0) as i32;
        let yd = yesterday.unwrap_or(0) as i32;
        let mut diff = 0;
        if (yd < td)  {
            diff = td - yd;
        }
        return diff as u32;
    }

    // TODO: Should return a Result<String, Err> so we can pass up an error from the expression parser
    fn custom_chart(&self, data: &Vec<DailyStats>, title: String, expression: String) -> String {
        let mut x = Vec::new();
        let mut y = Vec::new();
        let mut my_data = data.clone();

        // TODO: Should be separated logically and update when new data is downloaded
        my_data.reverse();
        let mut desummed_data =  Vec::new();

        for i in 1..my_data.len() - 1 {
            let today = my_data.get(i).unwrap();
            let yesterday = my_data.get(i-1).unwrap();

            let pos_diff = self.safe_diff(today.positive, yesterday.positive);
            let neg_diff = self.safe_diff(today.negative, yesterday.negative);
            let death_diff = self.safe_diff(today.death, yesterday.death);
            let hosp_diff = self.safe_diff(today.hospitalized, yesterday.hospitalized);
            let pending_diff = self.safe_diff(today.pending, yesterday.pending);
            let total_diff = self.safe_diff(today.total, yesterday.total);

            let desummed_daily = DailyStats {
                state: None,
                date: today.date,
                positive: Some(pos_diff),
                negative: Some(neg_diff),
                death: Some(death_diff),
                hospitalized: Some(hosp_diff),
                pending: Some(pending_diff),
                total: Some(total_diff)
            };
            desummed_data.push(desummed_daily);
        }

        for i in 0..(desummed_data.len() - 1) {
            let today = desummed_data.get(i).unwrap();

            // TODO: Data should be better sanitized before reaching charting logic
            let positives = today.positive.unwrap_or(0);
            let total = today.total.unwrap_or(0);
            let deaths = today.death.unwrap_or(0);
            let hospitalized = today.hospitalized.unwrap_or(0);
            let negatives = today.negative.unwrap_or(0);

            // Interpolate variable references provided by user with actual values
            let mut interp_exp = expression.replace("positive", &format!("{}", positives));
            interp_exp = interp_exp.replace("total", &format!("{}", total));
            interp_exp = interp_exp.replace("negative", &format!("{}", negatives));
            interp_exp = interp_exp.replace("hospitalized", &format!("{}", hospitalized));
            interp_exp = interp_exp.replace("dead", &format!("{}", deaths));

            // Try to parse and evaluate the expression
            let result = mexprp::eval::<f64>(&interp_exp);
            match result {
                Ok(res) => {
                    let results = res.to_vec();
                    y.push(*results.get(0).unwrap() as f32);
                },
                Err(err) => {
                    // TODO: Send error message to slack (or return an error to be handled by caller)
                    // TODO: check error logs first, might happen a lot with noisy data (divide by 0 especially)
                    println!("Failed to evaluate expression {:}", err);
                    y.push(0.0);

                }
            }

            // X axis (for now, always date)
            let date = NaiveDate::parse_from_str(&today.date.unwrap().to_string(), "%Y%m%d").unwrap();
            let t = NaiveTime::from_hms(0, 0, 0);
            let dt = date.and_time(t);
            x.push(dt.timestamp());

        }
        let y2: Vec<f32>  = Vec::new();
        let url = self.generate_chart(x, y, y2, &title, "", &expression, "");
        return url;
    }

    fn generate_new_cases_chart(&self,  data: &Vec<DailyStats>, title: String) -> String {
        let mut x = Vec::new();
        let mut y = Vec::new();
        let mut y2 = Vec::new();
        let mut my_data = data.clone();
        my_data.reverse();
        for i in 6..(my_data.len() - 1) {

            let mut total_diff = 0;
            for k in i-5..i {
                let today = my_data.get(k).unwrap();
                let yesterday = my_data.get(k-1).unwrap();

                // N.B. This handles bad Florida data that appears to show number of positive cases decreasing (which is impossible)
                let mut diff = 0;
                if (yesterday.positive.unwrap() <= today.positive.unwrap()) {
                    diff = today.positive.unwrap() - yesterday.positive.unwrap();
                }
                total_diff += diff;
            }
            let avg_diff = total_diff as f32 / 5.0;


            y.push(avg_diff);

            // TODO: Macro for this noisy code?
            let total_pos = (my_data.get(i).unwrap().positive.unwrap_or(0) as f32) - (my_data.get(i-5).unwrap().positive.unwrap_or(0) as f32);
            let total_tested = (my_data.get(i).unwrap().total.unwrap_or(0) as f32) - (my_data.get(i-5).unwrap().total.unwrap_or(0) as f32);

            let mut infection_rate = (total_pos / total_tested) * 100.0;

            // Clean up some noisy data observed in NY
            // Not ideal but hopefully helps a little
            if infection_rate < 0.0 || infection_rate > 50.0 {
                infection_rate = 0.0;
            }
            y2.push(infection_rate);
            let daily = my_data.get(i).unwrap();
            let date = NaiveDate::parse_from_str(&daily.date.unwrap().to_string(), "%Y%m%d").unwrap();
            let t = NaiveTime::from_hms(0, 0, 0);
            let dt = date.and_time(t);
            x.push(dt.timestamp());
        }
        let url = self.generate_chart(x, y, y2, &title, "", "Positives", "% Positive (trailing 5 days)");
        return url;
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

        let mut pos_change = 0.0;
        let mut neg_change = 0.0;
        let mut hosp_change = 0.0;
        let mut tested_change = 0.0;
        let mut dead_change = 0.0;

        let yesterday_el = data.get(1);

        match yesterday_el {
            Some(el) => {
                let yesterday_total_tested = el.positive.unwrap_or(0) + el.negative.unwrap_or(0);
                pos_change = (((total_positive - el.positive.unwrap_or(0)) as f64 / el.positive.unwrap_or(0) as f64) * 100.0);
                neg_change = (((total_negative - el.negative.unwrap_or(0)) as f64 / el.negative.unwrap_or(0) as f64) * 100.0);
                hosp_change = (((total_hospitalized - el.hospitalized.unwrap_or(0)) as f64 / el.hospitalized.unwrap_or(0) as f64) * 100.0);
                tested_change = (((total_tested - yesterday_total_tested) as f64 / yesterday_total_tested as f64) * 100.0);
                dead_change = (((total_dead - el.death.unwrap_or(0)) as f64 / el.death.unwrap_or(0) as f64) * 100.0);
            },
            None => {}
        }

        // If data is unreported or actually 0, % change will be undefined (0/0) but let's just say it's zero because it makes sense intuitively
        if hosp_change.is_nan() {
            hosp_change = 0.0;
        }
        if dead_change.is_nan() {
            dead_change = 0.0;
        }

        return format!("
        {geo_title} Overall Daily Stats ({date})\n \
        Total positive: {total_positive} (+{pos_change:.0}%)\n \
        Total negative: {total_negative} (+{neg_change:.0}%)\n \
        Total tested: {total_tested} (+{tested_change:.0}%)\n \
        Total hospitalized: {total_hospitalized} (+{hosp_change:.0}%)\n \
        Mortality rate: {death_rate:.2}% \n \
        Souls lost: {total_dead} (+{dead_change:.0}%)",
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

                // New UI stuff
                let spl: Vec<&str> = text.split_whitespace().collect();

                if spl.len() > 1 && *spl.get(1).unwrap() == "help" {
                    let to_send = "Usage:\n \
                    Overall new positive cases: @coronabot latest\
                    \nState new positive cases: @coronabot <state abbreviation>\
                    \nCustom chart (beta): @coronabot custom <state abbreviation> y1 <expression>\
                    \nCustom charts are aware of these variables: positive, negative, total, dead, hospitalized, pending. If you reference them in the expression, they will be interpolated into the expression. For example (positive/total) for infection rate.";
                    cli.sender().send_message(&channel, &to_send);
                    return;
                }

                if spl.len() > 3 && *spl.get(1).unwrap() == "custom" {
                    let state = spl.get(2).unwrap();
                    let exp_start = text.find("y1");

                    match exp_start {
                        Some(_) => {},
                        None => {
                            let to_send = "Missing y-axis specifier. Usage: @coronabot custom <state> y1 <expression>";
                            cli.sender().send_message(&channel, &to_send);
                            return;
                        }
                    }

                    let exp = &text[exp_start.unwrap()+3..text.len()];
                    println!("State: {:?} Exp: {:?}", state, exp);

                    let state_stats = self.states_daily.read().unwrap();
                    match &*state_stats {
                        Some(data) => {
                            // TODO: fix &state.to_string()
                            if !data.contains_key(&state.to_string()) {
                                let to_send = format!("State data is present but does not contain stats for {state}", state=state);
                                cli.sender().send_message(&channel, &to_send);
                                return;
                            }
                            // TODO: fix &state.to_string()
                            let state_data = data.get(&state.to_string()).unwrap();
                            let chart_url = self.custom_chart(state_data, format!("{state} Custom Chart", state=state),exp.to_string());
                            let mut to_send = String::new();
                            to_send.push_str("\n");
                            to_send.push_str(&chart_url);
                            cli.sender().send_message(&channel, &to_send);
                            return;
                        },
                        None => {
                            let to_send = "Sorry, state-level data is missing. Is the API working?";
                            cli.sender().send_message(&channel, &to_send);
                            return;
                        }
                    }
                }
                println!("Splt: {:?}", spl);

                //  Old UI stuff
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
                            let mut to_send = self.format_daily(data, "U.S.");
                            let chart_url = self.generate_new_cases_chart(data, "U.S. Coronavirus Cases".to_string());
                            to_send.push_str("\n");
                            to_send.push_str(&chart_url);
                            println!("Sending data");
                            cli.sender().send_message(&channel, &to_send);
                        },
                        None => {
                            let to_send = "Sorry, country-level data is missing. Is the API working?";
                            cli.sender().send_message(&channel, &to_send);
                        }
                    }
                } else if query == "help" {
                    let to_send = "Usage: @Coronabot <help|latest|<2 letter state abbreviation>|top>";
                    cli.sender().send_message(&channel, &to_send);
                } else if query == "top" {
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
                            let chart_url = self.generate_new_cases_chart(state_data, format!("{state} Coronavirus Cases", state=query));
                            let mut to_send = self.format_daily(state_data, &query);
                            to_send.push_str("\n");
                            to_send.push_str(&chart_url);
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