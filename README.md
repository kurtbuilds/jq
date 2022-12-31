<div id="top"></div>

<p align="center">
<a href="https://github.com/kurtbuilds/jq/graphs/contributors">
    <img src="https://img.shields.io/github/contributors/kurtbuilds/jq.svg?style=flat-square" alt="GitHub Contributors" />
</a>
<a href="https://github.com/kurtbuilds/jq/stargazers">
    <img src="https://img.shields.io/github/stars/kurtbuilds/jq.svg?style=flat-square" alt="Stars" />
</a>
<a href="https://github.com/kurtbuilds/jq/actions">
    <img src="https://img.shields.io/github/actions/workflow/status/kurtbuilds/jq/test.yaml?style=flat-square" alt="Build Status" />
</a>

</p>

# jq

Replacement for `stedolan/jq`. Why?

- `csv` subcommand to succinctly and intuitively convert JSON to CSV
- Handle multiple JSON documents in a single stream. `stedolan/jq` only handles one document at a time.
  This feature enables chaining multiple `jq` commands together, eliminating the need for quoting
  commands, as is the case with the `stedolan/jq`.

# Example

Here's an example that shows chaining of `jq` commands, and the `csv` subcommand.

```bash
cat data/reviews.json | jq feed.entry | jq csv content.label author.name.label > ios-reviews.csv
# Yes, this is an unnecessary use of cat :) It keeps the command order same as stream order.
```

# Installation

```bash
cargo install --git https://github.com/kurtbuilds/jq
```

### Why did you name it the same as `stedolan/jq`?

It's meant to be a drop-in replacement. Rather than use an alias, I just call the executable the same. On my machine, with `brew install jq`, I have both `jq` commands installed:

```bash
$ which -a jq
~/.cargo/bin/jq
/opt/homebrew/bin/jq
```

I use the fully qualified path `/opt/homebrew/bin/jq` if I need the `stedolan/jq` version for some reason.


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
