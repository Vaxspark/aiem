use axum::extract::State;
use axum::routing::get;
use axum::Router;
use maud::{html, Markup};

use crate::layout::{page, page_header, settings_group};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/ides", get(index))
}

async fn index(State(_st): State<AppState>) -> Markup {
    page(
        "IDEs",
        "/ides",
        html! {
            (page_header("IDEs", "Supported editors \u{2014} deploy targets for skills and MCP.", html!{}))
            div class="content-padding wide-content" {
                (settings_group("", html!{
                    table class="aiem" {
                        thead { tr { th{"ID"} th{"Display name"} th{"Skills directory"} th{"Default scope"} } }
                        tbody {
                            @for ide in aiem_core::ide::IDES {
                                tr {
                                    td class="mono" { (ide.id) }
                                    td style="font-weight:500" { (ide.display_name) }
                                    td class="mono meta" { (ide.skills_dir) }
                                    td class="meta" { (format!("{:?}", ide.default_scope)) }
                                }
                            }
                        }
                    }
                }))
            }
        },
    )
}
