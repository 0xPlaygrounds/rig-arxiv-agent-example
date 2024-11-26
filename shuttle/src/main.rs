use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

use rig::{
    completion::Prompt,
    providers::openai::{self, GPT_4},
};
use std::fmt::Write as _;
use tools::{ArxivSearchTool, Paper};

mod tools;

fn format_papers_as_table(papers: Vec<Paper>) -> Result<String, std::fmt::Error> {
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

async fn summarize_research(
    State(openai_client): State<openai::Client>,
) -> Result<String, AppError> {
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

    let papers: Vec<Paper> = serde_json::from_str(&response)?;
    Ok(format_papers_as_table(papers)?)
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    // Initialize OpenAI client
    let openai_client = openai::Client::from_env();

    let router = Router::new()
        .route("/", get(summarize_research))
        .with_state(openai_client);

    Ok(router.into())
}

//////////////////////
/// Error Handling ///
//////////////////////
struct AppError(anyhow::Error);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}
