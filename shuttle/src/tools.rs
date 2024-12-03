use core::str;
use std::fmt::Write;
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

// HTML formatting function for papers
pub fn format_papers_as_html(papers: &[Paper]) -> Result<String, std::fmt::Error> {
    let mut output = String::new();

    // Write table header
    writeln!(&mut output, "<div class='research-results'>")?;
    writeln!(&mut output, "<table class='papers-table'>")?;
    writeln!(
        &mut output,
        "<thead><tr><th>Title</th><th>Authors</th><th>Categories</th><th>URL</th></tr></thead>"
    )?;
    writeln!(&mut output, "<tbody>")?;

    // Write each paper's information
    for paper in papers {
        // Format authors
        let authors = if paper.authors.len() > 2 {
            format!("{} et al.", paper.authors[0])
        } else {
            paper.authors.join(", ")
        };

        writeln!(
            &mut output,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><a href='{}' target='_blank' class='paper-link'>View Paper</a></td></tr>",
            paper.title,
            authors,
            paper.categories.join(", "),
            paper.url
        )?;
    }

    writeln!(&mut output, "</tbody></table>")?;

    // Add abstract section
    writeln!(&mut output, "<div class='abstracts-section'>")?;
    writeln!(&mut output, "<h2>Paper Abstracts</h2>")?;
    for paper in papers {
        writeln!(&mut output, "<div class='abstract-container'>")?;
        writeln!(&mut output, "<h3>{}</h3>", paper.title)?;
        writeln!(&mut output, "<p><strong>Authors:</strong> {}</p>", paper.authors.join(", "))?;
        writeln!(&mut output, "<p><strong>Abstract:</strong></p>")?;
        writeln!(&mut output, "<p>{}</p>", paper.abstract_text)?;
        writeln!(&mut output, "<p><strong>Categories:</strong> {}</p>", paper.categories.join(", "))?;
        writeln!(&mut output, "<p><a href='{}' class='paper-link'>View paper</a></p>", paper.url)?;
        writeln!(&mut output, "</div>")?;
    }
    writeln!(&mut output, "</div></div>")?;

    Ok(output)
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
                                let url = str::from_utf8(&attr.value)?;
                                // Convert to HTTPS and ensure PDF URL
                                let secure_url = if url.contains("arxiv.org/abs/") {
                                    // Convert abstract URL to PDF URL
                                    url.replace("arxiv.org/abs/", "arxiv.org/pdf/")
                                       .replace("http://", "https://") + ".pdf"
                                } else if url.contains("arxiv.org/pdf/") {
                                    // Ensure PDF URL uses HTTPS
                                    url.replace("http://", "https://")
                                } else {
                                    // Fallback for other URLs
                                    url.replace("http://", "https://")
                                };
                                secure_url.clone_into(&mut paper.url);
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