use axum::{
    extract::Path,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

const REACT: &str = "vendor/react-19.3.1.js";

pub async fn module(Path(path): Path<String>, headers: HeaderMap) -> Response {
    let script = match path.as_str() {
        "core/api.js" => include_str!("../web/ui/core/api.js"),
        "core/poll.js" => include_str!("../web/ui/core/poll.js"),
        "core/view.js" => include_str!("../web/ui/core/view.js"),
        "navigation.js" => include_str!("../web/ui/navigation.js"),
        "catalog.js" => include_str!("../web/ui/catalog.js"),
        "features/models.js" => include_str!("../web/ui/features/models.js"),
        "features/images.js" => include_str!("../web/ui/features/images.js"),
        "features/certificates.js" => include_str!("../web/ui/features/certificates.js"),
        "features/logs.js" => include_str!("../web/ui/features/logs.js"),
        "features/runtime-options.js" => include_str!("../web/ui/features/runtime-options.js"),
        "features/generation.js" => include_str!("../web/ui/features/generation.js"),
        "features/tasks.js" => include_str!("../web/ui/features/tasks.js"),
        "features/record-editor.js" => include_str!("../web/ui/features/record-editor.js"),
        REACT => include_str!("../web/ui/vendor/react-19.3.1.js"),
        "vendor/LICENSES.txt" => {
            return (
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                include_str!("../web/ui/vendor/LICENSES.txt"),
            )
                .into_response();
        }
        _ => return StatusCode::NOT_FOUND.into_response(),
    };

    let compress = path == REACT && accepts_gzip(&headers);
    let mut response = if compress {
        (
            [(header::CONTENT_ENCODING, "gzip")],
            include_bytes!("../web/ui/vendor/react-19.3.1.js.gz").as_slice(),
        )
            .into_response()
    } else {
        script.into_response()
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("text/javascript; charset=utf-8"),
    );
    if path == REACT {
        headers.insert(
            header::VARY,
            header::HeaderValue::from_static("Accept-Encoding"),
        );
        headers.insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }
    response
}

fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|entry| {
            let mut fields = entry.split(';').map(str::trim);
            fields
                .next()
                .is_some_and(|name| name.eq_ignore_ascii_case("gzip"))
                && fields
                    .filter_map(|field| field.split_once('='))
                    .filter(|(name, _)| name.trim().eq_ignore_ascii_case("q"))
                    .all(|(_, weight)| {
                        weight
                            .trim()
                            .parse::<f32>()
                            .is_ok_and(|value| value > 0.0 && value <= 1.0)
                    })
        })
}
