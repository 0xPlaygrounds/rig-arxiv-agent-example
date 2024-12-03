use axum::{
    extract::{State, Json},
    response::{IntoResponse, Response, Html},
    routing::{get, post},
    Router,
};

use tower_http::cors::{CorsLayer, Any};

use rig::{
    completion::Prompt,
    providers::openai::{self, GPT_4},
};
use std::fmt::Write as _;
use tools::{ArxivSearchTool, Paper};
use serde::Deserialize;

mod tools;

#[derive(Deserialize)]
struct SearchRequest {
    query: String,
}

fn format_papers_as_table(papers: Vec<Paper>) -> Result<String, std::fmt::Error> {
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
    for paper in papers.iter() {
        // Format authors
        let authors = if paper.authors.len() > 2 {
            format!("{} et al.", paper.authors[0])
        } else {
            paper.authors.join(", ")
        };

        writeln!(
            &mut output,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><a href='{}' target='_blank'>View Paper</a></td></tr>",
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
    for (_i, paper) in papers.iter().enumerate() {
        writeln!(&mut output, "<div class='abstract-container'>")?;
        writeln!(&mut output, "<h3>{}</h3>", paper.title)?;
        writeln!(&mut output, "<p><strong>Authors:</strong> {}</p>", paper.authors.join(", "))?;
        writeln!(&mut output, "<p><strong>Abstract:</strong></p>")?;
        writeln!(&mut output, "<p>{}</p>", paper.abstract_text)?;
        writeln!(&mut output, "<p><strong>Categories:</strong> {}</p>", paper.categories.join(", "))?;
        writeln!(&mut output, "<p><a href='{}' target='_blank'>View on arXiv</a></p>", paper.url)?;
        writeln!(&mut output, "</div>")?;
    }
    writeln!(&mut output, "</div></div>")?;

    Ok(output)
}

async fn serve_index() -> impl IntoResponse {
    Html(include_str!("../static/index.html"))
}

async fn search_papers(
    State(openai_client): State<openai::Client>,
    Json(request): Json<SearchRequest>,
) -> Result<impl IntoResponse, AppError> {
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

    // Search for papers based on the query
    let response = paper_agent
        .prompt(&request.query)
        .await?;

    let papers: Vec<Paper> = serde_json::from_str(&response)?;
    Ok(Html(format_papers_as_table(papers)?))
}

#[shuttle_runtime::main]
async fn main() -> shuttle_axum::ShuttleAxum {
    // Initialize OpenAI client
    let openai_client = openai::Client::from_env();

    // Configure CORS
    let cors = CorsLayer::new()
        .allow_origin(Any) // Allow any origin for development
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers(Any);

    let router = Router::new()
        .route("/", get(serve_index))
        .route("/api/search", post(search_papers))
        .layer(cors)
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