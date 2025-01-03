# arXiv Scraping Agent Example hosted on Shuttle
This example shows how you can run an AI agent that uses arXiv to help you learn about a given subject.

## How to use
To get started, you will need an OpenAI API key.

1) Use `shuttle init --from 0xplaygrounds/rig-arxiv-agent-ai-example --subfolder shuttle` to clone this repository.
2) Create a `Secrets.toml` file and place your OpenAI API key accordingly (see `Secrets.toml.example` if unsure)
3) Use `shuttle run` to run the program if you'd like to try it locally before deploying. By default, local runs use port 8000.
4) Use `shuttle deploy` to deploy!

Once deployed, you will recieve a URL which you can use to access your newly deployed webservice.
