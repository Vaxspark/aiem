use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use maud::{html, Markup};
use serde::Deserialize;

use aiem_core::registry::{self, RegistryItem, RegistrySource};

use crate::layout::{btn_primary, card, empty_state, page, page_header, tag, TagKind};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/store", get(index))
        .route("/store/search", get(search))
}

#[derive(Deserialize, Default)]
struct SearchQ {
    #[serde(default)]
    q: String,
}

async fn index(State(_st): State<AppState>) -> Markup {
    let items = registry::popular().await.unwrap_or_default();
    page("Store", "/store", html!{
        (page_header("Store", "Search online registries (smithery.ai · glama.ai · anthropic skills).", html!{}))
        (card(html!{
            form hx-get="/store/search" hx-target="#store-results" hx-swap="innerHTML" class="flex gap-2 items-end" {
                div style="flex:1" { label class="label" { "Query" } input name="q" placeholder="filesystem, playwright, …" class="field"; }
                (btn_primary("Search"))
            }
        }))
        div id="store-results" { (render(&items)) }
    })
}

async fn search(Query(q): Query<SearchQ>) -> Response {
    let items = if q.q.is_empty() {
        registry::popular().await.unwrap_or_default()
    } else {
        registry::search_all(&q.q).await.unwrap_or_default()
    };
    render(&items).into_response()
}

fn render(items: &[RegistryItem]) -> Markup {
    if items.is_empty() {
        return empty_state("No results", "Try a different keyword or check your connection.");
    }
    card(html!{
        table class="aiem" {
            thead { tr { th{"Name"} th{"Source"} th{"Uses"} th{"Description"} th style="text-align:right"{"Link"} } }
            tbody {
                @for i in items {
                    tr {
                        td style="font-weight:500" { (i.name) }
                        td { (tag(i.source.label(), TagKind::Neutral)) }
                        td class="meta" { (i.use_count) }
                        td class="meta line-clamp-2" { (i.description) }
                        td style="text-align:right;white-space:nowrap" {
                            a href=(i.url) target="_blank" rel="noopener" class="btn-ghost" { "Open →" }
                            @if let Some(gh) = &i.github {
                                a href=(gh) target="_blank" rel="noopener" class="btn-ghost" { "GitHub" }
                            }
                        }
                    }
                }
            }
        }
    })
}

#[allow(dead_code)]
fn _unused_source(_: RegistrySource) {}
