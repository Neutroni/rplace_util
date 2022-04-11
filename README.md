# rplace_util
Simple program to anyze reddit r/place 2022 contributions

Program takes a single parameter on the command line which is the location of
the config.toml file
config.toml contains the locations you want to search for contributions
and the setting for when to end the search and the locations of the csv data file
By default user must have placed pixels in all the selected areas and not
anywhere else, if not all locations are know removal of users who have edits
outside selected areas can be disabled.

See sample in the repository for example for the config file.
