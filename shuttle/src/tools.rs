use core::str;

use quick_xml::{events::Event, Reader};
use rig::{completion::ToolDefinition, tool::Tool};
use serde_json::json;

const ARXIV_URL: &str = "http://export.arxiv.org/api/query";

#[derive(Debug, thiserror::Error)]
pub enum ArxivError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("XML parsing error: {0}")]
    XmlParsing(#[from] quick_xml::Error),
    #[error("No results found")]
    NoResults,
    #[error("UTF-8 decoding error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
}

// Struct to hold paper metadata
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Paper {
    pub title: String,
    pub authors: Vec<String>,
    pub abstract_text: String,
    pub url: String,
    pub categories: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct SearchArgs {
    query: String,
    max_results: Option<i32>,
}

// Tool to search for papers
#[derive(serde::Deserialize, serde::Serialize)]
pub struct ArxivSearchTool;

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

        let response = client
            .get(ARXIV_URL)
            .query(&[
                ("search_query", format!("all:{}", args.query)),
                ("start", 0.to_string()),
                ("max_results", max_results.to_string()),
            ])
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
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
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
            },
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
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"href" {
                                str::from_utf8(&attr.value)?.clone_into(&mut paper.url);
                            }
                        }
                    }
                }
                if in_entry && e.name().as_ref() == b"category" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"term" {
                            current_categories.push(str::from_utf8(&attr.value)?.to_owned());
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"entry" => {
                    if let Some(mut paper) = current_paper.take() {
                        paper.authors.clone_from(&current_authors);
                        paper.categories.clone_from(&current_categories);
                        papers.push(paper);
                    }
                    in_entry = false;
                }
                b"title" | b"author" | b"summary" | b"link" | b"category" => {
                    current_field = None;
                }
                _ => (),
            },
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
