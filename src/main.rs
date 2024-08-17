use std::{error::Error, fs, io};

use csv::Writer;
use pest::Parser;
use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "grammar.pest"]
pub struct SQLParser;

fn main() -> Result<(), Box<dyn Error>> {
    let lines = io::stdin().lines();
    let mut csv_stdout_writer = Writer::from_writer(io::stdout());

    for line in lines {
        let line = line?;

        // only read insert statements
        if !line.starts_with("INSERT") {
            continue;
        }

        let pairs =
            SQLParser::parse(Rule::insert_statement, &line).map_err(|e| {
                fs::write("line.txt", e.line()).unwrap();
                format!(
                    "{:?} {:?} {:?}",
                    e.variant,
                    e.line_col,
                    e.location
                )
            })?;
        // .map_err(|e| format!("{:?}", e.line_col))?;

        for database_row in pairs {
            if database_row.as_rule() == Rule::EOI {
                continue;
            }

            let record =
                database_row
                    .into_inner()
                    .map(|value| match value.as_rule() {
                        Rule::string => value.as_str(),
                        Rule::number => value.as_str(),
                        Rule::null => "",
                        _ => unreachable!("Expected integer or string"),
                    });

            csv_stdout_writer.write_record(record)?;
        }

        csv_stdout_writer.flush()?;
    }

    Ok(())
}
