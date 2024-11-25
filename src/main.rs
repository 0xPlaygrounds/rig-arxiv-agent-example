use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;
use rig::{
    completion::{Prompt, ToolDefinition},
    providers::openai::{self, GPT_4},
    tool::Tool,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::str;
use std::fmt::Write as _;

// Struct to hold paper metadata
#[derive(Debug, Deserialize, Serialize)]
struct Paper {
    title: String,
    authors: Vec<String>,
    abstract_text: String,
    url: String,
    categories: Vec<String>,
}

// Tool to search for papers
#[derive(Deserialize, Serialize)]
struct ArxivSearchTool;

#[derive(Deserialize)]
struct SearchArgs {
    query: String,
    max_results: Option<i32>,
}

#[derive(Debug, thiserror::Error)]
enum ArxivError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("XML parsing error: {0}")]
    XmlParsing(#[from] quick_xml::Error),
    #[error("No results found")]
    NoResults,
    #[error("UTF-8 decoding error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
}

impl Tool for ArxivSearchTool {
    const NAME: &'static str = "search_arxiv";
    type Error = ArxivError;
    type Args = SearchArgs;
    type Output = Vec<Paper>;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "search_arxiv".to_string(),
            description: "Search for academic papers on arXiv".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query for papers"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 5)"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let max_results = args.max_results.unwrap_or(5);
        let client = reqwest::Client::new();
        
        // URL encode the query
        let encoded_query = urlencoding::encode(&args.query);
        
        // Construct arXiv API URL
        let url = format!(
            "http://export.arxiv.org/api/query?search_query=all:{}&start=0&max_results={}", 
            encoded_query,
            max_results
        );

        let response = client.get(&url)
            .send()
            .await?
            .text()
            .await?;

        parse_arxiv_response(&response)
    }
}

fn parse_arxiv_response(response: &str) -> Result<Vec<Paper>, ArxivError> {
    let mut reader = Reader::from_str(response);
    reader.trim_text(true);

    let mut papers = Vec::new();
    let mut current_paper: Option<Paper> = None;
    let mut current_authors = Vec::new();
    let mut current_categories = Vec::new();
    let mut buf = Vec::new();
    let mut in_entry = false;
    let mut current_field = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"entry" => {
                        in_entry = true;
                        current_paper = Some(Paper {
                            title: String::new(),
                            authors: Vec::new(),
                            abstract_text: String::new(),
                            url: String::new(),
                            categories: Vec::new(),
                        });
                        current_authors.clear();
                        current_categories.clear();
                    }
                    b"title" if in_entry => current_field = Some("title"),
                    b"author" if in_entry => current_field = Some("author"),
                    b"summary" if in_entry => current_field = Some("abstract"),
                    b"link" if in_entry => current_field = Some("link"),
                    b"category" if in_entry => current_field = Some("category"),
                    _ => (),
                }
            }
            Ok(Event::Text(e)) => {
                if let Some(paper) = current_paper.as_mut() {
                    let text = str::from_utf8(e.as_ref())?.to_owned();
                    match current_field {
                        Some("title") => paper.title = text,
                        Some("author") => current_authors.push(text),
                        Some("abstract") => paper.abstract_text = text,
                        _ => (),
                    }
                }
            }
            Ok(Event::Empty(ref e)) => {
                if in_entry && e.name().as_ref() == b"link" {
                    if let Some(paper) = current_paper.as_mut() {
                        for attr in e.attributes() {
                            if let Ok(attr) = attr {
                                if attr.key.as_ref() == b"href" {
                                    paper.url = str::from_utf8(&attr.value)?.to_owned();
                                }
                            }
                        }
                    }
                }
                if in_entry && e.name().as_ref() == b"category" {
                    for attr in e.attributes() {
                        if let Ok(attr) = attr {
                            if attr.key.as_ref() == b"term" {
                                current_categories.push(str::from_utf8(&attr.value)?.to_owned());
                            }
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                match e.name().as_ref() {
                    b"entry" => {
                        if let Some(mut paper) = current_paper.take() {
                            paper.authors = current_authors.clone();
                            paper.categories = current_categories.clone();
                            papers.push(paper);
                        }
                        in_entry = false;
                    }
                    b"title" | b"author" | b"summary" | b"link" | b"category" => {
                        current_field = None;
                    }
                    _ => (),
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(ArxivError::XmlParsing(e)),
            _ => (),
        }
    }

    if papers.is_empty() {
        return Err(ArxivError::NoResults);
    }

    Ok(papers)
}

fn format_papers_as_table(papers_json: &str) -> Result<String, anyhow::Error> {
    let papers: Vec<Paper> = serde_json::from_str(papers_json)?;
    
    let mut output = String::new();
    
    // Write table header
    writeln!(&mut output, "\n{:-^120}", " Research Papers ")?;
    writeln!(
        &mut output,
        "{:<50} | {:<20} | {:<15} | {:<30}",
        "Title", "Authors", "Categories", "URL"
    )?;
    writeln!(&mut output, "{:-<120}", "")?;

    // Write each paper's information
    for paper in papers.iter() {
        // Truncate and format title
        let title = if paper.title.len() > 47 {
            format!("{}...", &paper.title[..47])
        } else {
            paper.title.clone()
        };

        // Format authors
        let authors = if paper.authors.len() > 2 {
            format!("{} et al.", paper.authors[0])
        } else {
            paper.authors.join(", ")
        };
        let authors = if authors.len() > 17 {
            format!("{}...", &authors[..17])
        } else {
            authors
        };

        // Format categories
        let categories = paper.categories.join(", ");
        let categories = if categories.len() > 12 {
            format!("{}...", &categories[..12])
        } else {
            categories
        };

        // Format URL
        let url = if paper.url.len() > 27 {
            format!("{}...", &paper.url[..27])
        } else {
            paper.url.clone()
        };

        writeln!(
            &mut output,
            "{:<50} | {:<20} | {:<15} | {:<30}",
            title, authors, categories, url
        )?;
    }

    // Add abstract section
    writeln!(&mut output, "\n{:-^120}", " Abstracts ")?;
    for (i, paper) in papers.iter().enumerate() {
        writeln!(&mut output, "\n{}. {}", i + 1, paper.title)?;
        writeln!(&mut output, "Authors: {}", paper.authors.join(", "))?;
        writeln!(&mut output, "\nAbstract:\n{}\n", paper.abstract_text)?;
        writeln!(&mut output, "Categories: {}\n", paper.categories.join(", "))?;
        writeln!(&mut output, "URL: {}\n", paper.url)?;
        writeln!(&mut output, "{:-<120}", "")?;
    }

    Ok(output)
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Initialize OpenAI client
    let openai_client = openai::Client::from_env();

    // Create agent with arxiv search tool
    let paper_agent = openai_client
        .agent(GPT_4)
        .preamble(
            "You are a helpful research assistant that can search and analyze academic papers from arXiv. \
             When asked about a research topic, use the search_arxiv tool to find relevant papers and \
             return only the raw JSON response from the tool."
        )
        .tool(ArxivSearchTool)
        .build();

    // Example usage
    let response = paper_agent
        .prompt("Find recent papers about large language models and summarize them")
        .await?;

    // Format and print the table
    match format_papers_as_table(&response) {
        Ok(formatted_table) => println!("{}", formatted_table),
        Err(e) => println!("Error formatting table: {}", e),
    }

    Ok(())
}