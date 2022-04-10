use std::collections::{HashMap, HashSet};
use std::{env, io};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, Write};
use config::Config;
use log::{error, info, warn};
use nom::IResult;
use nom::bytes::complete::{take_until};
use nom::branch::alt;
use nom::character::complete;
use nom::sequence::{delimited, terminated};
use serde::Deserialize;

#[derive(Eq, PartialEq, Hash, Deserialize)]
struct TileLocation {
    x: u16,
    y: u16,
}

impl TileLocation {
    fn parse(input: &str) -> IResult<&str, LineCoordinate> {
        let (input, x) = terminated(complete::u16, complete::char(','))(input)?;
        let (input, y) = complete::u16(input)?;
        Ok((input, LineCoordinate::Tile(TileLocation {
            x,
            y,
        })))
    }
}

#[derive(Eq, PartialEq, Hash, Deserialize)]
struct TileRegion {
    start: TileLocation,
    end: TileLocation,
}

impl TileRegion {
    fn parse_line(input: &str) -> IResult<&str, LineCoordinate> {
        //1349,1718,1424,1752
        let (input, start_x) = terminated(complete::u16, complete::char(','))(input)?;
        let (input, start_y) = terminated(complete::u16, complete::char(','))(input)?;
        let (input, end_x) = terminated(complete::u16, complete::char(','))(input)?;
        let (input, end_y) = complete::u16(input)?;
        Ok((input, LineCoordinate::Region(TileRegion {
            start: TileLocation {
                x: start_x,
                y: start_y,
            },
            end: TileLocation {
                x: end_x,
                y: end_y,
            },
        })))
    }

    fn contains(&self, location: &TileLocation) -> bool {
        if location.x < self.start.x {
            return false;
        }
        if location.y < self.start.y {
            return false;
        }
        if location.x > self.end.x {
            return false;
        }
        if location.y > self.end.y {
            return false;
        }
        true
    }

    fn intersects(&self, region: &TileRegion) -> bool {
        self.contains(&region.start) || self.contains(&region.end)
            || region.contains(&self.start) || region.contains(&self.end)
    }
}

#[derive(Eq, PartialEq, Hash)]
enum LineCoordinate {
    Tile(TileLocation),
    Region(TileRegion),
}

impl LineCoordinate {
    fn parse(input: &str) -> IResult<&str, LineCoordinate> {
        alt((TileRegion::parse_line, TileLocation::parse))(input)
    }
}

struct CanvasLine {
    timestamp: String,
    user_id: String,
    pixel_color: String,
    coordinate: LineCoordinate,
}

impl CanvasLine {
    fn parse(input: &str) -> IResult<&str, CanvasLine> {
        //2022-04-04 00:55:57.168 UTC,tPcrtm7OtEmSThdRSWmB7jmTF9lUVZ1pltNv1oKqPY9bom/EGIO3/b5kjRenbD3vMF48psnR9MnhIrTT1bpC9A==,#6A5CFF,"1908,1854"
        let (input, timestamp) = terminated(take_until(","), complete::char(','))(input)?;
        //tPcrtm7OtEmSThdRSWmB7jmTF9lUVZ1pltNv1oKqPY9bom/EGIO3/b5kjRenbD3vMF48psnR9MnhIrTT1bpC9A==,#6A5CFF,"1908,1854"
        let (input, user_id) = terminated(take_until(","), complete::char(','))(input)?;
        //#6A5CFF,"1908,1854"
        let (input, pixel_color) = terminated(take_until(","), complete::char(','))(input)?;
        //"1908,1854" or "1349,1718,1424,1752"
        let (input, coordinate) = delimited(complete::char('"'), LineCoordinate::parse, complete::char('"'))(input)?;

        Ok((input, CanvasLine {
            timestamp: timestamp.to_string(),
            user_id: user_id.to_string(),
            pixel_color: pixel_color.to_string(),
            coordinate,
        }))
    }
}

#[derive(Deserialize)]
struct Settings {
    csv_location: String,
    end_time: String,
    search_areas: Vec<TileRegion>,
}

fn main() {
    //Init logger
    env_logger::init();

    //Get config file location from command line
    let has_config_path;
    let args: Vec<String> = env::args().collect();
    let config_path = if args.len() > 1 {
        has_config_path = true;
        &args[1]
    } else {
        has_config_path = false;
        "config.toml"
    };

    let config = Config::builder()
        .set_default("csv_location", "2022_place_canvas_history.csv")
        .expect("Failed to set default csv location")
        .set_default("end_time", "2022-04-04 21:32:37.541 UTC")
        .expect("Failed to set default end time")
        .add_source(config::File::with_name(config_path).required(has_config_path))
        .build()
        .expect("Configuration file contains errors");
    let settings: Settings = config.try_deserialize()
        .expect("Failed to parse configurations");

    //Get list of potential users in selected areas
    let potential_users = find_users_in_area(&settings.search_areas, &settings.csv_location);
    if potential_users.is_empty() {
        println!("Did not find any users.");
        return;
    }

    println!("Found users:");
    for (index, user) in potential_users.iter().enumerate() {
        println!("{}: {}", index, user);
    }

    print!("Select user by giving index: ");
    if let Err(e) = io::stdout().flush() {
        error!("Failed to flush stdout: {}", e);
    }
    let input;
    loop {
        let mut user_input = String::new();
        if let Err(e) = io::stdin().read_line(&mut user_input) {
            error!("Failed to read input: {}", e);
            continue;
        }
        //Remove trailing newline
        user_input.pop();
        let user_index: usize = match user_input.parse() {
            Ok(v) => {
                if v >= potential_users.len() {
                    eprintln!("Index out of bounds");
                    continue;
                }
                v
            }
            Err(_) => {
                eprintln!("Give zero based index of user you want to select");
                continue;
            }
        };
        input = user_index;
        break;
    }

    let user_hash = &potential_users[input];
    find_remaining_tiles(user_hash, &settings.csv_location, &settings.end_time);
}

/**
 * Find users who only placed tiles at locations specified in locations
 */
fn find_users_in_area(locations: &Vec<TileRegion>, file_name: &str) -> Vec<String> {
    let mut file = File::open(file_name).expect("Failed to open tile data");
    let reader = BufReader::new(&file);

    //Iterate over rows to find ALL users who placed tiles inside locations
    let mut users = HashMap::<&TileRegion, HashSet<String>>::new();
    let mut line_reader = reader.lines();
    if line_reader.next().is_none() {
        panic!("Could not skip CSV header");
    };
    for line_result in line_reader {
        let line = match line_result {
            Ok(l) => { l }
            Err(e) => {
                warn!("Failed to obtain line from tile data: {}", e);
                continue;
            }
        };
        let row_result = match CanvasLine::parse(&line) {
            Ok((_, v)) => { v }
            Err(_) => {
                warn!("Malformed line in data: {}", line);
                continue;
            }
        };

        for location in locations {
            match &row_result.coordinate {
                LineCoordinate::Tile(t) => {
                    if location.contains(t) {
                        let user_list = users.entry(location).or_insert_with(|| { HashSet::<String>::new() });
                        user_list.insert(row_result.user_id.clone());
                        break;
                    }
                }
                LineCoordinate::Region(r) => {
                    if location.intersects(r) {
                        let user_list = users.entry(location).or_insert_with(|| { HashSet::<String>::new() });
                        user_list.insert(row_result.user_id.clone());
                        break;
                    }
                }
            }
        }
    }

    //New reader to read from start
    if let Err(e) = file.rewind() {
        panic!("Could not rewind file: {}", e);
    }
    let reader = BufReader::new(&file);
    let mut line_reader = reader.lines();
    if line_reader.next().is_none() {
        panic!("Could not skip CSV header");
    };
    for line_result in line_reader {
        let line = match line_result {
            Ok(l) => { l }
            Err(e) => {
                warn!("Failed to obtain line from tile data: {}", e);
                continue;
            }
        };
        let row_result = match CanvasLine::parse(&line) {
            Ok((_, v)) => { v }
            Err(_) => {
                warn!("Malformed line in data: {}", line);
                continue;
            }
        };

        //Remove users who have edits outside locations
        let mut is_outside = true;
        for location in locations {
            match &row_result.coordinate {
                LineCoordinate::Tile(t) => {
                    if location.contains(t) {
                        is_outside = false;
                        break;
                    }
                }
                LineCoordinate::Region(r) => {
                    if location.intersects(r) {
                        is_outside = false;
                        break;
                    }
                }
            }
        }
        //Edit is not in any selected area
        if is_outside {
            for location in locations {
                if let Some(v) = users.get_mut(location) {
                    v.remove(&row_result.user_id);
                }
            }
        }
    }

    //Calculate intersections of all users
    let mut iterator = users.values();
    let mut first = match iterator.next() {
        None => { panic!("No regions selected!") }
        Some(v) => { v.clone() }
    };
    for i in iterator {
        let mut new_set = HashSet::<String>::new();
        for x in first.intersection(i) {
            new_set.insert(x.clone());
        }
        first = new_set;
    }

    let mut results = Vec::<String>::new();
    for i in first {
        results.push(i);
    }
    results
}

/**
 * Get tiles user had before whitening occurred on the image
 */
fn find_remaining_tiles(user_hash: &str, file_name: &str, end_time: &str) {
    let file = File::open(file_name).expect("Failed to open tile data");
    let reader = BufReader::new(file);
    let mut remaining_coordinates: HashMap<TileLocation, String> = HashMap::new();
    let mut line_reader = reader.lines();
    if line_reader.next().is_none() {
        panic!("Could not skip CSV header");
    };
    for line_result in line_reader {
        let line = match line_result {
            Ok(l) => { l }
            Err(e) => {
                warn!("Failed to obtain line from tile data: {}", e);
                continue;
            }
        };

        let row_result = match CanvasLine::parse(&line) {
            Ok((_, v)) => { v }
            Err(_) => {
                warn!("Malformed line in data: {}", line);
                continue;
            }
        };

        //Check that user is one who we want
        if row_result.user_id == user_hash {
            //Current user, add to tiles
            match row_result.coordinate {
                LineCoordinate::Tile(t) => {
                    info!("Found {} tile placed at: {},{}", row_result.pixel_color, t.x, t.y);
                    remaining_coordinates.insert(t, row_result.pixel_color.clone());
                }
                LineCoordinate::Region(r) => {
                    for i in r.start.x..r.end.x {
                        for j in r.start.y..r.end.y {
                            remaining_coordinates.insert(TileLocation {
                                x: i,
                                y: j,
                            }, row_result.pixel_color.clone());
                        }
                    }
                }
            }
        } else {
            //Was not current user, remove from tiles if present
            match row_result.coordinate {
                LineCoordinate::Tile(t) => {
                    remaining_coordinates.remove(&t);
                }
                LineCoordinate::Region(r) => {
                    for i in r.start.x..r.end.x {
                        for j in r.start.y..r.end.y {
                            remaining_coordinates.remove(&TileLocation {
                                x: i,
                                y: j,
                            });
                        }
                    }
                }
            }
        }

        //Stop searching before whitening
        if row_result.timestamp == end_time {
            break;
        }
    }

    for (i, color) in &remaining_coordinates {
        println!("Remaining {} tile: {},{}", color, i.x, i.y);
    }

    if remaining_coordinates.is_empty() {
        println!("No tiles remaining");
    }
}
