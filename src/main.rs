use std::collections::{HashMap, HashSet};
use std::{env, io};
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use config::Config;
use env_logger::Env;
use rayon::iter::ParallelBridge;
use rayon::prelude::ParallelIterator;
use log::{error, info, warn};
use nom::{bytes, IResult};
use nom::bytes::complete::{take_until};
use nom::branch::alt;
use nom::character::complete;
use nom::combinator::eof;
use nom::sequence::{delimited, terminated};
use serde::Deserialize;
use time::format_description::FormatItem;
use time::PrimitiveDateTime;

#[derive(Eq, PartialEq, Hash, Deserialize, Clone)]
struct TileLocation {
    x: i16,
    y: i16,
}

impl Display for TileLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}, {}", self.x, self.y)
    }
}

impl TileLocation {
    fn parse(input: &str) -> IResult<&str, LineCoordinate> {
        let (input, x) = terminated(complete::i16, complete::char(','))(input)?;
        let (input, y) = complete::i16(input)?;
        Ok((input, LineCoordinate::Tile(TileLocation {
            x,
            y,
        })))
    }
}

#[derive(Eq, PartialEq, Hash, Deserialize, Clone)]
struct TileRegion {
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
}

impl TileRegion {
    fn parse_line(input: &str) -> IResult<&str, LineCoordinate> {
        //1349,1718,1424,1752
        let (input, start_x) = terminated(complete::i16, complete::char(','))(input)?;
        let (input, start_y) = terminated(complete::i16, complete::char(','))(input)?;
        let (input, end_x) = terminated(complete::i16, complete::char(','))(input)?;
        let (input, end_y) = complete::i16(input)?;
        Ok((input, LineCoordinate::Region(TileRegion {
            left: start_x,
            top: start_y,
            right: end_x,
            bottom: end_y,
        })))
    }

    fn contains(&self, location: &TileLocation) -> bool {
        self.contains_point(location.x, location.y)
    }

    fn contains_point(&self, x: i16, y: i16) -> bool {
        if x < self.left {
            return false;
        }
        if y < self.top {
            return false;
        }
        if x > self.right {
            return false;
        }
        if y > self.bottom {
            return false;
        }
        true
    }

    fn intersects(&self, region: &TileRegion) -> bool {
        self.contains_point(region.left, region.top)
            || self.contains_point(region.right, region.top)
            || self.contains_point(region.right, region.bottom)
            || self.contains_point(region.left, region.bottom)
            || region.contains_point(self.left, self.top)
            || region.contains_point(self.right, self.top)
            || region.contains_point(self.right, self.bottom)
            || region.contains_point(self.left, self.bottom)
    }
}

#[derive(Eq, PartialEq, Hash, Deserialize, Clone)]
struct TileCircle {
    x: i16,
    y: i16,
    r: i16,
}

impl TileCircle {
    fn parse_line(input: &str) -> IResult<&str, LineCoordinate> {
        //{X: 425, Y: 441, R: 2}
        let (input, _) = bytes::complete::tag("{X: ")(input)?;
        let (input, x) = terminated(complete::i16, complete::char(','))(input)?;
        let (input, _) = bytes::complete::tag(" Y: ")(input)?;
        let (input, y) = terminated(complete::i16, complete::char(','))(input)?;
        let (input, _) = bytes::complete::tag(" R: ")(input)?;
        let (input, r) = terminated(complete::i16, complete::char('}'))(input)?;
        Ok((input, LineCoordinate::Circle(TileCircle {
            x,
            y,
            r,
        })))
    }

    fn contains_point(&self, x: i16, y: i16) -> bool {
        let x_dist = (x - self.x) as f64;
        let y_dist = (y - self.y) as f64;
        let dist = f64::sqrt((x_dist * x_dist) + (y_dist * y_dist));
        dist < self.r as f64
    }

    fn intersects(&self, region: &TileRegion) -> bool {
        self.contains_point(region.left, region.top)
            || self.contains_point(region.right, region.top)
            || self.contains_point(region.right, region.bottom)
            || self.contains_point(region.left, region.bottom)
            || region.contains_point(self.x + self.r, self.y + self.r)
            || region.contains_point(self.x - self.r, self.y - self.r)
            || region.contains_point(self.x + self.r, self.y - self.r)
            || region.contains_point(self.x - self.r, self.y + self.r)
    }

    fn points(&self) -> Vec<TileLocation> {
        let mut pixels: Vec<TileLocation> = Vec::new();

        let mut x = self.r;
        let mut y = 0;
        let mut radius_error = 1 - x;

        while x >= y
        {
            let mut start_x = -x + self.x;
            let mut end_x = x + self.y;
            self.line_points(&mut pixels, start_x, end_x, y + self.y);
            if y != 0 {
                self.line_points(&mut pixels, start_x, end_x, -y + self.y);
            }
            y += 1;

            if radius_error < 0 {
                radius_error += 2 * y + 1;
            } else {
                if x >= y {
                    start_x = -y + 1 + self.x;
                    end_x = y - 1 + self.x;
                    self.line_points(&mut pixels, start_x, end_x, x + self.y);
                    self.line_points(&mut pixels, start_x, end_x, -x + self.y);
                }
                x -= 1;
                radius_error += 2 * (y - x + 1);
            }
        }
        pixels
    }

    fn line_points(&self, pixels: &mut Vec<TileLocation>, from_x: i16, to_x: i16, y: i16) {
        for x in from_x..to_x {
            pixels.push(TileLocation {
                x,
                y,
            });
        }
    }
}

#[derive(Eq, PartialEq, Hash)]
enum LineCoordinate {
    Tile(TileLocation),
    Region(TileRegion),
    Circle(TileCircle),
}

impl LineCoordinate {
    fn parse(input: &str) -> IResult<&str, LineCoordinate> {
        alt((TileRegion::parse_line, TileLocation::parse, TileCircle::parse_line))(input)
    }
}

struct CanvasLine {
    timestamp: String,
    user_id: String,
    pixel_color: String,
    coordinate: LineCoordinate,
}

impl CanvasLine {
    fn parse2022(input: &str) -> IResult<&str, CanvasLine> {
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
    fn parse2023(input: &str) -> IResult<&str, CanvasLine> {
        //2023-07-20 18:55:57.168 UTC,tPcrtm7OtEmSThdRSWmB7jmTF9lUVZ1pltNv1oKqPY9bom/EGIO3/b5kjRenbD3vMF48psnR9MnhIrTT1bpC9A==,"1908,1854",#6A5CFF
        let (input, timestamp) = terminated(take_until(","), complete::char(','))(input)?;
        //tPcrtm7OtEmSThdRSWmB7jmTF9lUVZ1pltNv1oKqPY9bom/EGIO3/b5kjRenbD3vMF48psnR9MnhIrTT1bpC9A==,"1908,1854",#6A5CFF
        let (input, user_id) = terminated(take_until(","), complete::char(','))(input)?;
        //"1908,1854" or "1349,1718,1424,1752" or "{X: 481, Y: 416, R: 3}"
        let (input, coordinate) = delimited(complete::char('"'), LineCoordinate::parse, complete::char('"'))(input)?;
        //#6A5CFF
        let (input, pixel_color) = delimited(complete::char(','), bytes::complete::take(7usize), eof)(input)?;

        Ok((input, CanvasLine {
            timestamp: timestamp.to_string(),
            user_id: user_id.to_string(),
            pixel_color: pixel_color.to_string(),
            coordinate,
        }))
    }
}

time::serde::format_description!(rplace_time_format, PrimitiveDateTime, "[year]-[month]-[day] [hour]:[minute]:[second].[subsecond] UTC");

#[derive(Deserialize, Clone)]
struct SearchArea {
    #[serde(with = "rplace_time_format::option", default)]
    start_time: Option<PrimitiveDateTime>,
    #[serde(with = "rplace_time_format::option", default)]
    end_time: Option<PrimitiveDateTime>,
    #[serde(default)]
    is_optional: bool,
    #[serde(default)]
    colours: Vec<String>,
    area: TileRegion,
}

impl SearchArea {
    fn contains(&self, pixel: &CanvasLine) -> bool {
        const RPLACE_TIME_FORMAT: &[FormatItem] = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second].[subsecond] UTC");
        const RPLACE_TIME_FORMAT_SHORT: &[FormatItem] = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second] UTC");

        let line_time = PrimitiveDateTime::parse(&pixel.timestamp, RPLACE_TIME_FORMAT)
            .or_else(|_| {
                PrimitiveDateTime::parse(&pixel.timestamp, RPLACE_TIME_FORMAT_SHORT)
            })
            .unwrap_or_else(|_| panic!("Can not parse: {} Malformed time in CSV", &pixel.timestamp));
        if let Some(start_time) = self.start_time {
            if start_time < line_time {
                return false;
            }
        }
        if let Some(end_time) = self.end_time {
            if line_time > end_time {
                return false;
            }
        }
        if !self.colours.is_empty() && !self.colours.contains(&pixel.pixel_color) {
            return false;
        }

        match &pixel.coordinate {
            LineCoordinate::Tile(t) => {
                self.area.contains(t)
            }
            LineCoordinate::Region(r) => {
                self.area.intersects(r)
            }
            LineCoordinate::Circle(c) => {
                c.intersects(&self.area)
            }
        }
    }
}

#[derive(Deserialize)]
enum PlaceDataSet {
    Place2023,
    Place2022,
}

#[derive(Deserialize)]
struct Settings {
    user_id: Option<String>,
    csv_location: String,
    year: PlaceDataSet,
    search_areas: Vec<SearchArea>,
    no_edits_outside: bool,
}

fn main() {
    //Init logger
    env_logger::Builder::from_env(Env::default().default_filter_or("info"))
        .format(|buf, record| {
            writeln!(buf, "[{}] {}", record.level(), record.args())
        })
        .init();

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
        .set_default("no_edits_outside", true)
        .expect("Failed to set default no_edits_outside")
        .add_source(config::File::with_name(config_path).required(has_config_path))
        .build()
        .expect("Configuration file contains errors");
    let settings: Settings = config.try_deserialize()
        .expect("Failed to parse configurations");

    //If we do not have a user id try to find user from specified areas
    let settings_user = settings.user_id.clone();
    let userid = settings_user.or_else(|| {
        find_user(&settings)
    });
    //Check if we have a user id
    if let Some(user) = userid {
        info!("Finding tiles that remain");
        find_remaining_tiles(&user, &settings.csv_location, &settings.year);
    }
}

fn find_user(settings: &Settings) -> Option<String> {
    //HashMap of users who have edits in selected areas
    let users = Arc::new(Mutex::new(
        HashMap::<String, HashSet<TileRegion>>::new()));

    //Get list of potential users in selected areas
    let locations = &settings.search_areas;
    info!("Finding users who have edits in selected areas");
    mutate_user_list(add_internal_edits, locations, &settings.csv_location, users.clone(), &settings.year);
    match users.lock() {
        Ok(g) => {
            info!("Total users in selected area {}", g.len())
        }
        Err(e) => {
            eprintln!("Mutex lock failed: {}", e);
        }
    }

    //If enabled remove users who have edits outside selected areas
    if settings.no_edits_outside {
        info!("Removing users who have edits outside selected areas");
        mutate_user_list(remove_external_edits, locations, &settings.csv_location, users.clone(), &settings.year);
        match users.lock() {
            Ok(g) => {
                info!("After removing users who have edits outside selected areas {}", g.len())
            }
            Err(e) => {
                eprintln!("Mutex lock failed: {}", e);
            }
        }
    }

    //Set of search areas that user must be present in
    let required_ares: HashSet<TileRegion> = locations.iter().filter(|a| {
        !a.is_optional
    }).map(|r| {
        r.area.clone()
    }).collect();

    //Remove uses who did not have edits in all selected areas
    let user = match users.lock() {
        Ok(mut g) => {
            //Remove elements which were not found in all selected areas
            info!("Removing users who do not have edits in all selected areas");
            g.retain(|_, regions| {
                regions.is_superset(&required_ares)
            });
            let potential_users: Vec<String> = g.clone().into_keys().collect();
            if potential_users.is_empty() {
                println!("Did not find any users.");
                return None;
            }

            println!("Found users:");
            for (index, user) in potential_users.iter().enumerate() {
                println!("{}: {}", index, user);
            }

            let input;
            if potential_users.len() > 1 {
                print!("Select user by giving index: ");
                if let Err(e) = io::stdout().flush() {
                    error!("Failed to flush stdout: {}", e);
                }
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
            } else {
                input = 0;
            }

            Some(potential_users[input].clone())
        }
        Err(e) => {
            eprintln!("Mutex lock failed: {}", e);
            None
        }
    };
    user
}

/**
 * Add users who have edits inside selected areas to the HashMap
 */
fn add_internal_edits(users: Arc<Mutex<HashMap<String, HashSet<TileRegion>>>>, line: &str, locations: &Vec<SearchArea>, year: &PlaceDataSet) {
    //Convert line to struct
    let row_result = match year {
        PlaceDataSet::Place2023 => {
            match CanvasLine::parse2023(line) {
                Ok((_, v)) => { v }
                Err(_) => {
                    warn!("Malformed line in data: {}", line);
                    return;
                }
            }
        }
        PlaceDataSet::Place2022 => {
            match CanvasLine::parse2022(line) {
                Ok((_, v)) => { v }
                Err(_) => {
                    warn!("Malformed line in data: {}", line);
                    return;
                }
            }
        }
    };

    //Check if coordinates in selected areas
    for location in locations {
        //Check if search area matches the line
        if !location.contains(&row_result) {
            continue;
        }
        //Matches, add area to the set of areas user has placed pixels in
        match users.lock() {
            Ok(mut g) => {
                let region_set = g.entry(row_result.user_id.clone())
                    .or_insert_with(|| { HashSet::<TileRegion>::new() });
                region_set.insert(location.area.clone());
            }
            Err(e) => {
                eprintln!("Mutex lock failed: {}", e);
            }
        }
    }
}

/**
 * Remove users who have edits outside selected areas from the HashMap
 */
fn remove_external_edits(users: Arc<Mutex<HashMap<String, HashSet<TileRegion>>>>, line: &str, locations: &Vec<SearchArea>, year: &PlaceDataSet) {
    let row_result = match year {
        PlaceDataSet::Place2023 => {
            match CanvasLine::parse2023(line) {
                Ok((_, v)) => { v }
                Err(_) => {
                    warn!("Malformed line in data: {}", line);
                    return;
                }
            }
        }
        PlaceDataSet::Place2022 => {
            match CanvasLine::parse2022(line) {
                Ok((_, v)) => { v }
                Err(_) => {
                    warn!("Malformed line in data: {}", line);
                    return;
                }
            }
        }
    };

    //Remove users who have edits outside locations
    let mut is_outside = true;
    for location in locations {
        match &row_result.coordinate {
            LineCoordinate::Tile(t) => {
                if location.area.contains(t) {
                    is_outside = false;
                    break;
                }
            }
            LineCoordinate::Region(r) => {
                if location.area.intersects(r) {
                    is_outside = false;
                    break;
                }
            }
            LineCoordinate::Circle(c) => {
                if c.intersects(&location.area) {
                    is_outside = false;
                    break;
                }
            }
        }
    }
    //Edit is not in any selected area
    if is_outside {
        match users.lock() {
            Ok(mut g) => {
                g.remove(&row_result.user_id);
            }
            Err(e) => {
                eprintln!("Mutex lock failed: {}", e);
            }
        }
    }
}

/**
 * Function that calls the supplied function on the rows of the text file in a thread
 */
fn mutate_user_list<F: 'static>(update_func: F, locations: &Vec<SearchArea>, file_name: &str, users: Arc<Mutex<HashMap<String, HashSet<TileRegion>>>>, year: &PlaceDataSet)
    where F: Fn(Arc<Mutex<HashMap<String, HashSet<TileRegion>>>>, &str, &Vec<SearchArea>, &PlaceDataSet) + Send + Sync + Copy {
    let file = File::open(file_name)
        .expect("Failed to open tile data");
    let reader = BufReader::new(&file);

    //Iterate over rows to find ALL users who placed tiles inside locations
    let mut line_reader = reader.lines();
    if line_reader.next().is_none() {
        panic!("Could not skip CSV header");
    };

    line_reader.par_bridge().for_each(|line_result| {
        match line_result {
            Ok(l) => {
                update_func(users.clone(), &l, locations, year);
            }
            Err(e) => {
                warn!("Failed to obtain line from tile data: {}", e);
            }
        };
    });
}

/**
 * Get surviving tiles
 */
fn find_remaining_tiles(user_hash: &str, file_name: &str, year: &PlaceDataSet) {
    let file = File::open(file_name).expect("Failed to open tile data");
    let reader = BufReader::new(file);

    const WHITEOUT_LINE_2023: usize = 126816301;
    const WHITEOUT_LINE_2022: usize = 158117508;
    let whiteout_line: usize = match year {
        PlaceDataSet::Place2023 => { WHITEOUT_LINE_2023 }
        PlaceDataSet::Place2022 => { WHITEOUT_LINE_2022 }
    };
    let mut reached_whiteout = false;
    //Number of tiles user has placed
    let mut tiles_placed: u64 = 0;
    //Tiles that made it to the start of whiteout
    let mut whiteout_tiles: HashMap<TileLocation, String> = HashMap::new();
    //Tiles that made it to the end
    let mut end_tiles: HashMap<TileLocation, String> = HashMap::new();

    let mut line_reader = reader.lines().enumerate();
    if line_reader.next().is_none() {
        panic!("Could not skip CSV header");
    };
    for (line_number, line_result) in line_reader {
        if line_number == whiteout_line {
            reached_whiteout = true;
        }

        let line = match line_result {
            Ok(l) => { l }
            Err(e) => {
                warn!("Failed to obtain line from tile data: {}", e);
                continue;
            }
        };

        let row_result = match year {
            PlaceDataSet::Place2023 => {
                match CanvasLine::parse2023(&line) {
                    Ok((_, v)) => { v }
                    Err(_) => {
                        warn!("Malformed line in data: {}", line);
                        continue;
                    }
                }
            }
            PlaceDataSet::Place2022 => {
                match CanvasLine::parse2022(&line) {
                    Ok((_, v)) => { v }
                    Err(_) => {
                        warn!("Malformed line in data: {}", line);
                        continue;
                    }
                }
            }
        };

        //Check that user is one who we want
        if row_result.user_id == user_hash {
            //Current user, add to tiles
            tiles_placed += 1;
            //Check if the tile is a region
            match row_result.coordinate {
                LineCoordinate::Tile(t) => {
                    info!("Found {} Color: {} tile placed at: {},{}", row_result.timestamp, row_result.pixel_color, t.x, t.y);
                    //Add tiles that could have survived to the whiteout
                    if !reached_whiteout {
                        whiteout_tiles.insert(t.clone(), row_result.pixel_color.clone());
                    }
                    //Add tiles that could have survived to the end
                    end_tiles.insert(t, row_result.timestamp.clone());
                }
                LineCoordinate::Region(r) => {
                    for i in r.left..r.right {
                        for j in r.top..r.bottom {
                            let tile_location = TileLocation {
                                x: i,
                                y: j,
                            };
                            //Remove tiles that did not survive to the whiteout
                            if !reached_whiteout {
                                whiteout_tiles.insert(tile_location.clone(), row_result.pixel_color.clone());
                            }
                            //Remove tiles that did not survive to the end
                            end_tiles.insert(tile_location, row_result.timestamp.clone());
                        }
                    }
                }
                LineCoordinate::Circle(c) => {
                    let points = c.points();
                    for p in points {
                        //Remove tiles that did not survive to the whiteout
                        if !reached_whiteout {
                            whiteout_tiles.insert(p.clone(), row_result.pixel_color.clone());
                        }
                        //Remove tiles that did not survive to the end
                        end_tiles.insert(p, row_result.timestamp.clone());
                    }
                }
            }
        } else {
            //Was not current user, remove from tiles if present
            match row_result.coordinate {
                LineCoordinate::Tile(t) => {
                    if !reached_whiteout {
                        whiteout_tiles.remove(&t);
                    }
                    end_tiles.remove(&t);
                }
                LineCoordinate::Region(r) => {
                    for i in r.left..r.right {
                        for j in r.top..r.bottom {
                            whiteout_tiles.remove(&TileLocation {
                                x: i,
                                y: j,
                            });
                        }
                    }
                }
                LineCoordinate::Circle(c) => {
                    let points = c.points();
                    for p in points {
                        whiteout_tiles.remove(&p);
                    }
                }
            }
        }
    }

    //Print the number of tiles user placed
    println!("User placed  {} tiles total", tiles_placed);

    //Print out all the tiles that made it to the start of whiteout
    if whiteout_tiles.is_empty() {
        println!("No tiles on the final image");
    } else {
        println!("Following tiles made it to the final image:");
    }

    for (location, color) in &whiteout_tiles {
        println!("{} at: {}", color, location);
    }


    //Print out all the tiles that made it to  the end
    if end_tiles.is_empty() {
        println!("No tiles survived to the end");
    } else {
        println!("Following tiles made it to the end:")
    }

    for (location, time) in &end_tiles {
        println!("{} placed at {}", location, time);
    }
}
