# jq

Replacement for `jq`. Why?

- `csv` command to succinctly and intuitively convert JSON to CSV
- Handle multiple JSON documents in a single stream. `stedolan/jq` only handles one document at a time.
  This feature enables chaining multiple `jq` commands together, eliminating the need for quoting
  commands, as is the case with the `stedolan/jq`.

# Example

Here's an example that shows chaining of `jq` commands, and the `csv` subcommand.

    cat data/reviews.json | jq feed.entry | jq csv content.label author.name.label > ios-reviews.csv
    # Yes, this is an unnecessary use of cat :) It keeps the command order same as stream order.
  
### Differences compared to `stedolan/jq`

- leading `.` is optional
- bash command chaining works, so we don't have to wrap the command in single quotes `''`
- Just pass keypaths into the `csv` command to generate a csv. No esoteric command syntax.
- Strings are printed `raw` by default, not wrapped in quotes.

# Roadmap

- [x] Basic `jq` functionality
- [x] Chained documents
- [x] Csv subcommand
- [x] Colored output

# Contributions

Need features? Open an issue or a PR.
