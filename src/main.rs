use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::{error::Error, fs, io::BufRead};

use clap::Parser;
use deepsize::DeepSizeOf;
use indicatif::ParallelProgressIterator;
use itertools::Itertools;
use pest::iterators::Pair;
use rayon::prelude::*;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

type DatabaseRow = Vec<DatabaseEntry>;

#[derive(Debug, PartialEq)]
enum DatabaseEntry {
    Integer(i32),
    Float(f32),
    String(String),
    Null,
}

#[derive(pest_derive::Parser)]
#[grammar = "grammar.pest"]
pub struct SQLParser;

/// Extract records from from a SQL data dump
#[derive(clap::Parser, Debug)]
#[command(version, about, long_about = None)]
struct CommandLineArgs {
    /// SQL file from which to read. Supports gzipped
    /// files and reads from stdin if omitted.
    #[arg(short, long)]
    input_file: Option<String>,

    /// The number of database rows to extract. Defaults to all.
    #[arg(short, long)]
    n_rows: Option<usize>,
}

/// Extracts extract individual rows from insert statement,
/// e.g. `INSERT INTO xxxx VALUES (...), (...), ....;`
fn parse_insert_statement(line: &str) -> Vec<DatabaseRow> {
    use pest::Parser;
    let pairs = SQLParser::parse(Rule::insert_statement, &line)
        .map_err(|e| {
            std::fs::write("line.log", e.line()).unwrap();

            format!(
                "Failed to parse insert statement {:?} {:?} {:?}",
                e.line_col, e.location, e.variant,
            )
        })
        .unwrap();

    let rows = pairs
        .filter(|token_pair| token_pair.as_rule() == Rule::row)
        .map(move |token_pair| parse_db_row(token_pair));

    // PERF: collecting to vec here isn't ideal, but I think it's needed
    // if we want to accept a generic buffered input stream
    rows.collect_vec()
}

fn parse_db_row(token_pair: Pair<Rule>) -> DatabaseRow {
    use DatabaseEntry::*;

    let db_row = token_pair.into_inner(); // e.g. (1, 3, "asdf", NULL)
                                          //
    db_row
        .map(move |value| match value.as_rule() {
            Rule::integer => Integer(
                value.as_str().parse().expect("Failed to parse integer."),
            ),
            Rule::string => {
                // TODO: replace escaped quotes/other chars
                String(value.as_str().trim_matches('\'').replace("\\", ""))
            }
            Rule::float => {
                Float(value.as_str().parse().expect("Failed to parse float."))
            }
            Rule::null => Null,
            _ => {
                unreachable!("Expected integer, float, string, or NULL")
            }
        })
        .collect()
}

fn insert_statements(
    reader: impl BufRead + Send,
) -> impl Iterator<Item = String> + Send {
    reader
        .lines()
        .map(|line| line.expect("Failed to read line")) // unwrap lines
        .filter(|line| line.starts_with("INSERT"))
}

/// Construct appropriate input stream given the command line arguments,
/// decompressing as necessary.
fn get_input_stream(args: &CommandLineArgs) -> Result<Box<dyn BufRead + Send>> {
    let reader: Box<dyn Read + Send> = if let Some(fp) = &args.input_file {
        if fp.ends_with(".gz") {
            Box::new(flate2::read::GzDecoder::new(fs::File::open(fp)?))
        } else {
            Box::new(fs::File::open(fp)?)
        }
    } else {
        Box::new(std::io::stdin())
    };

    Ok(Box::new(BufReader::new(reader)))
}

fn multithread_db_rows(
    input_stream: impl BufRead + Send,
) -> impl ParallelIterator<Item = DatabaseRow> {
    insert_statements(input_stream)
        .par_bridge()
        .flat_map(|line| parse_insert_statement(&line))
}

fn main() -> Result<()> {
    let args = CommandLineArgs::parse();

    let input_stream = get_input_stream(&args)?;
    let n = args.n_rows.unwrap_or(usize::MAX);

    let pagelinks = create_pagelink_id_mapping(
        multithread_db_rows(input_stream).take_any(n),
    )?;

    println!("{}", pagelinks.capacity());
    println!("{}", pagelinks.deep_size_of());

    // multithread_db_rows(input_stream)
    //     .progress_count(n as u64)
    //     .take_any(n)
    //     .for_each(|row| println!("{:?}", row));

    Ok(())
}

// ----------------

fn create_pagelink_id_mapping(
    db_rows: impl ParallelIterator<Item = DatabaseRow>,
) -> Result<HashMap<i32, i32>> {
    use DatabaseEntry::*;

    let mapping = db_rows
        .map(|row| match row[..3] {
            [Integer(src_id), Integer(namespace), Integer(target_id)] => {
                (src_id, namespace, target_id)
            }
            _ => {
                panic!("Malformed row {:?}", row);
            }
        })
        .filter(|&(_, namespace, _)| namespace == 0)
        .map(|(src_id, _, target_id)| (src_id, target_id))
        .collect();

    Ok(mapping)
}

fn create_page_id_mapping(
    db_rows: impl ParallelIterator<Item = DatabaseRow>,
) -> Result<HashMap<i32, i32>> {
    use DatabaseEntry::*;

    let mapping = db_rows
        .map(|row| match row[..3] {
            [Integer(src_id), Integer(namespace), Integer(target_id)] => {
                (src_id, namespace, target_id)
            }
            _ => {
                panic!("Malformed row {:?}", row);
            }
        })
        .filter(|&(_, namespace, _)| namespace == 0)
        .map(|(src_id, _, target_id)| (src_id, target_id))
        .collect();

    Ok(mapping)
}
