# jq

Extract from json and transform into csv.

# Example

    cat data/reviews.json | jq feed.entry | jq csv content.label author.name.label > ios-reviews.csv
  
Note the following differences to `jq`:
- leading `.` is optional
- bash command chaining works, so we don't have to wrap the command in single quotes `''`
- Just pass keypaths into the `csv` command to generate a csv

Strings are not printed wrapped in quotes.

# TODO

- Colorize the output
