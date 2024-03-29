# rplace_util
Simple program to analyze reddit r/place 2022 or 2023 contributions

The program can search for users who edited the canvas in certain
areas or show info about edits of a user
The number of pixels placed total and info about pixels that made
it to the final image  and the pixels that survived to the actual
end of the r/place are shown

# Configuration
Program takes a single command line argument as the location of
a file used to configure the program, if no file is specified
default of 'config.toml' is used.
Configuration is done using TOML for which documentation can be
found at https://toml.io/

Sample configuration file is provided in the repository as `config.toml`

Configuration file needs to contain the location of the CSV file used to
store canvas edits, compressed archive of the 2022 file can be downloaded from 
[Reddit](https://placedata.reddit.com/data/canvas-history/2022_place_canvas_history.csv.gzip)
for 2023 split files can be found [here](https://placedata.reddit.com/canvas-history/) you need to
download all the files and combine uncompressed files to single file with headers only from the first file
* `csv_location` defines the location of the uncompressed CSV file
* `user_id` Which defines the hashed user id of the user we want to analyze
contributions for, if you do not know the user id hash program can find
potential users based on users who edited areas on the canvas
* `no_edits_outside` Which defines if users who have edits outside selected areas
should be removed from the list of potential users, default is 'true'
* `search_areas` is array of tables that defines the areas that are to be searched
    * `start_time` Optional, Defines the earliest time user can have edited a pixel in the search area 
    * `end_time` Optional, Defines the latest time user can have edited a pixel in the search area
    * `is_optional` Optional, Do not remove users who have edits in area but do not require edits in the area
    * `colours` Optional, Define colours which the edits can use, TOML string array
    * `area` Defines the edges of the area to search
        * `left` X-coordinate of the left edge of the search area
        * `top` Y-coordinate of the top edge of the search area
        * `right` X-coordinate of the right edge of the search area
        * `bottom` Y-coordinate of the bottom edge of the search area 