use super::*;
use axum::body::to_bytes;
use tower::ServiceExt;
use webauthn_authenticator_rs::{WebauthnAuthenticator, softpasskey::SoftPasskey};

pub(super) fn fixture() -> (tempfile::TempDir, Shared) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("content");
    fs::create_dir_all(root.join("media/uploads")).unwrap();
    let site = serde_json::json!({"schema_version":1,"title":"Test archive","base_url":"https://www.example.com","timezone":"America/Sao_Paulo","page_size":10,"categories":[{"id":"3","name":"Artigos","slug":"artigos","parent":"0"}],"authors":[{"id":"1","name":"Autor","slug":"autor"}],"navigation":[],"redirects":{},"expected_missing":[],"observed_paths":[],"banner":null});
    write_json(&root.join("site.json"), &site).unwrap();
    let metadata = serde_json::json!({"schema_version":1,"id":"wp-1","wordpress_id":"1","kind":"post","status":"published","title":"Original","published_at":"2026-10-16T15:00:00-03:00","modified_at":"2026-10-16T15:00:00-03:00","original_published_local":"2026-10-16 15:00:00","original_published_gmt":"2026-10-16 18:00:00","permalink":"/artigos/original/","categories":["artigos"],"tags":[],"author":"1","featured_image":null,"featured_image_alt":null,"featured_image_caption":null,"legacy_urls":[],"listings":[]});
    write_json(&root.join("articles/wp-1/metadata.json"), &metadata).unwrap();
    fs::write(root.join("articles/wp-1/body.html"), "<p>Original body</p>").unwrap();
    let state = temp.path().join("state");
    let db = database(&state).unwrap();
    for (key, value) in [
        ("user_id", Uuid::new_v4().to_string()),
        ("origin", "http://localhost:8781".into()),
        ("rp_id", "localhost".into()),
        ("recovery_hash", digest("offline-test-secret")),
    ] {
        db.execute("INSERT INTO settings VALUES(?1,?2)", params![key, value])
            .unwrap();
    }
    db.execute(
        "INSERT INTO bootstrap VALUES(1,?1,?2)",
        params![digest(&"a".repeat(64)), now() + 600],
    )
    .unwrap();
    drop(db);
    let app = make_app(
        root,
        &state,
        "http://localhost:8781".into(),
        "localhost".into(),
    )
    .unwrap();
    (temp, app)
}
async fn call(
    app: &Shared,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    csrf: Option<&str>,
    body: serde_json::Value,
) -> (StatusCode, HeaderMap, serde_json::Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8781")
        .header("origin", "http://localhost:8781")
        .header("content-type", "application/json");
    if let Some(cookie) = cookie {
        request = request.header("cookie", cookie);
    }
    if let Some(csrf) = csrf {
        request = request.header("x-csrf-token", csrf);
    }
    let response = router(app.clone())
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 20 * 1024 * 1024)
        .await
        .unwrap();
    (
        status,
        headers,
        serde_json::from_slice(&bytes).unwrap_or_default(),
    )
}
fn session_cookie(headers: &HeaderMap, name: &str) -> String {
    headers
        .get_all("set-cookie")
        .iter()
        .find_map(|h| {
            let h = h.to_str().unwrap();
            h.starts_with(&format!("{name}="))
                .then(|| h.split(';').next().unwrap().to_owned())
        })
        .unwrap()
}
fn authenticated(app: &Shared) -> (String, String) {
    let (headers, csrf) = app.establish().unwrap();
    (session_cookie(&headers, SESSION_COOKIE), csrf)
}

#[tokio::test]
async fn rejects_unauthenticated_access_csrf_and_foreign_origins() {
    let (_temp, app) = fixture();
    for path in [
        "/api/articles",
        "/api/articles/wp-1",
        "/api/deployments",
        "/api/analytics",
        "/api/credentials",
        "/api/media/uploads/x.png",
    ] {
        assert_eq!(
            call(&app, "GET", path, None, None, serde_json::json!({}))
                .await
                .0,
            StatusCode::UNAUTHORIZED
        );
    }
    for (path, body) in [
        (
            "/api/publish/wp-1",
            serde_json::json!({"revision":"test","expected_active":"baseline"}),
        ),
        (
            "/api/rollback",
            serde_json::json!({"release":"baseline","expected_active":"new"}),
        ),
    ] {
        assert_eq!(
            call(&app, "POST", path, None, None, body.clone()).await.0,
            StatusCode::UNAUTHORIZED
        );
        let (cookie, _) = authenticated(&app);
        assert_eq!(
            call(&app, "POST", path, Some(&cookie), None, body).await.0,
            StatusCode::FORBIDDEN
        );
    }
    let (cookie, csrf) = authenticated(&app);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/articles",
            Some(&cookie),
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let request = Request::builder()
        .method("POST")
        .uri("/api/articles")
        .header("host", "localhost:8781")
        .header("origin", "https://evil.example")
        .header("cookie", &cookie)
        .header("x-csrf-token", &csrf)
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(
        router(app.clone()).oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let request = Request::builder()
        .uri("/api/status")
        .header("host", "evil.example")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router(app.clone()).oneshot(request).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let (_, _, one) = call(
        &app,
        "GET",
        "/api/session",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    let (_, _, two) = call(
        &app,
        "GET",
        "/api/session",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(one["csrf"], two["csrf"]);
    assert_eq!(one["csrf"], csrf);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/logout",
            Some(&cookie),
            Some(&csrf),
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/articles",
            Some(&cookie),
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn draft_edits_are_durable_conflict_checked_and_never_change_published_input() {
    let (temp, app) = fixture();
    let (cookie, csrf) = authenticated(&app);
    let (_, _, original) = call(
        &app,
        "GET",
        "/api/articles/wp-1",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    let mut payload = serde_json::json!({"revision":original["revision"],"title":"Revised draft","html":"<p style='text-align:center'>Verse<br>Line</p><script>evil()</script>","permalink":"/artigos/original/","categories":["artigos"],"featured_image":null,"featured_image_alt":"","featured_image_caption":"","published_local":"2026-10-16T15:00:00"});
    let (status, _, saved) = call(
        &app,
        "PUT",
        "/api/articles/wp-1",
        Some(&cookie),
        Some(&csrf),
        payload.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(saved["metadata"]["status"], "draft");
    assert!(!saved["html"].as_str().unwrap().contains("<script"));
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/articles/wp-1",
            Some(&cookie),
            Some(&csrf),
            payload.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    payload["revision"] = saved["revision"].clone();
    payload["permalink"] = "/changed/".into();
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/articles/wp-1",
            Some(&cookie),
            Some(&csrf),
            payload
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let entries = load_entries(&temp.path().join("content")).unwrap();
    assert_eq!(entries[0].metadata.title, "Original");
    assert_eq!(entries[0].body, "<p>Original body</p>");
    let restored = Store::open(temp.path().join("content")).unwrap();
    assert_eq!(restored.entries["wp-1"].metadata.title, "Revised draft");
    assert_eq!(restored.entries["wp-1"].publication_status, "published");
    let (status, _, new) = call(
        &app,
        "POST",
        "/api/articles",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(new["metadata"]["wordpress_id"], "");
    assert_eq!(load_entries(&temp.path().join("content")).unwrap().len(), 1);
}

async fn send_file(
    app: &Shared,
    cookie: &str,
    csrf: &str,
    bytes: &[u8],
) -> (StatusCode, serde_json::Value) {
    let mut body=b"--testboundary\r\nContent-Disposition: form-data; name=\"image\"; filename=\"untrusted.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n".to_vec();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--testboundary--\r\n");
    let request = Request::builder()
        .method("POST")
        .uri("/api/uploads")
        .header("host", "localhost:8781")
        .header("origin", "http://localhost:8781")
        .header("cookie", cookie)
        .header("x-csrf-token", csrf)
        .header("content-type", "multipart/form-data; boundary=testboundary")
        .body(Body::from(body))
        .unwrap();
    let response = router(app.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1000000).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn image_uploads_are_decoded_and_preview_capabilities_expire_on_logout() {
    let (_temp, app) = fixture();
    let (cookie, csrf) = authenticated(&app);
    assert_eq!(
        send_file(&app, &cookie, &csrf, b"<svg onload='evil()'></svg>")
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    let mut image = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(4, 4)
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    let (status, result) = send_file(&app, &cookie, &csrf, &image.into_inner()).await;
    assert_eq!(status, StatusCode::OK);
    let path = result["path"].as_str().unwrap();
    assert!(path.ends_with(".png"));
    assert_eq!(
        call(
            &app,
            "GET",
            &format!("/api/media{path}"),
            None,
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let payload = serde_json::json!({"title":"Preview","html":format!("<p>Text</p><img src='{path}' onerror='evil()'><script>evil()</script>"),"featured_image":null,"featured_image_alt":"","featured_image_caption":""});
    let (status, _, preview) = call(
        &app,
        "POST",
        "/api/preview",
        Some(&cookie),
        Some(&csrf),
        payload,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let markup = preview["html"].as_str().unwrap();
    assert!(!markup.contains("<script"));
    assert!(!markup.contains("onerror"));
    let image_url = regex::Regex::new(r#"src="http://localhost:8781([^"]+)""#)
        .unwrap()
        .captures(markup)
        .unwrap()[1]
        .to_owned();
    assert_eq!(
        call(&app, "GET", &image_url, None, None, serde_json::json!({}))
            .await
            .0,
        StatusCode::OK
    );
    call(
        &app,
        "POST",
        "/api/logout",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        call(&app, "GET", &image_url, None, None, serde_json::json!({}))
            .await
            .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn webauthn_registration_login_and_replay_protection_use_real_signature_verification() {
    let (_temp, app) = fixture();
    let origin = url::Url::parse("http://localhost:8781").unwrap();
    // Test-only software authenticator and ephemeral database. Never installed as a staging credential.
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let (status, headers, flow) = call(
        &app,
        "POST",
        "/api/auth/register/start",
        None,
        None,
        serde_json::json!({"bootstrap":"a".repeat(64)}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cookie = session_cookie(&headers, FLOW_COOKIE);
    let options: CreationChallengeResponse =
        serde_json::from_value(flow["options"].clone()).unwrap();
    let credential = authenticator
        .do_registration(origin.clone(), options)
        .unwrap();
    let payload = serde_json::json!({"id":flow["id"],"credential":credential,"name":"Ephemeral test authenticator"});
    let (status, session_headers, _) = call(
        &app,
        "POST",
        "/api/auth/register/finish",
        Some(&cookie),
        None,
        payload.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(session_headers.get_all("set-cookie").iter().any(|h| {
        h.to_str()
            .unwrap()
            .contains("HttpOnly; Secure; SameSite=Strict")
    }));
    assert_eq!(count_credentials(&app.db.lock().unwrap()).unwrap(), 1);
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/auth/register/finish",
            Some(&cookie),
            None,
            payload
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (status, headers, flow) = call(
        &app,
        "POST",
        "/api/auth/login/start",
        None,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cookie = session_cookie(&headers, FLOW_COOKIE);
    let credential = authenticator
        .do_authentication(
            origin,
            serde_json::from_value(flow["options"].clone()).unwrap(),
        )
        .unwrap();
    let payload = serde_json::json!({"id":flow["id"],"credential":credential});
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/auth/login/finish",
        Some(&cookie),
        None,
        payload.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let authenticated = session_cookie(&headers, SESSION_COOKIE);
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/articles",
            Some(&authenticated),
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/auth/login/finish",
            Some(&cookie),
            None,
            payload
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn webauthn_rejects_mismatched_origin_and_expired_bootstrap() {
    let (_temp, app) = fixture();
    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let (_, headers, flow) = call(
        &app,
        "POST",
        "/api/auth/register/start",
        None,
        None,
        serde_json::json!({"bootstrap":"a".repeat(64)}),
    )
    .await;
    let cookie = session_cookie(&headers, FLOW_COOKIE);
    let credential = authenticator
        .do_registration(
            url::Url::parse("http://localhost:8782").unwrap(),
            serde_json::from_value(flow["options"].clone()).unwrap(),
        )
        .unwrap();
    let result = call(
        &app,
        "POST",
        "/api/auth/register/finish",
        Some(&cookie),
        None,
        serde_json::json!({"id":flow["id"],"credential":credential,"name":"Rejected"}),
    )
    .await;
    assert_eq!(result.0, StatusCode::BAD_REQUEST);
    assert_eq!(count_credentials(&app.db.lock().unwrap()).unwrap(), 0);
    app.db
        .lock()
        .unwrap()
        .execute("UPDATE bootstrap SET expires_at=0", [])
        .unwrap();
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/auth/register/start",
            None,
            None,
            serde_json::json!({"bootstrap":"a".repeat(64)})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn editorial_pages_tags_revision_restore_and_trash_are_durable() {
    let (_temp, app) = fixture();
    let (cookie, csrf) = authenticated(&app);
    let (status, _, page) = call(
        &app,
        "POST",
        "/api/articles",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"kind":"page"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["metadata"]["kind"], "page");
    assert_eq!(page["metadata"]["categories"], serde_json::json!([]));
    let id = page["metadata"]["id"].as_str().unwrap();
    let path = format!("/api/articles/{id}");
    let mut payload = serde_json::json!({"revision":page["revision"],"title":"Page one","html":"<p>First version</p>","permalink":"/test-page/","categories":[],"tags":["Espírito","Filosofia","Espírito"],"featured_image":null,"featured_image_alt":"","featured_image_caption":"","published_local":"2026-10-22T15:00:00"});
    let (status, _, saved) = call(
        &app,
        "PUT",
        &path,
        Some(&cookie),
        Some(&csrf),
        payload.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        saved["metadata"]["tags"],
        serde_json::json!(["Espírito", "Filosofia"])
    );
    payload["revision"] = saved["revision"].clone();
    payload["title"] = "Page two".into();
    payload["html"] = "<p>Second version</p>".into();
    let (_, _, second) = call(&app, "PUT", &path, Some(&cookie), Some(&csrf), payload).await;
    let (status, _, history) = call(
        &app,
        "GET",
        &format!("{path}/revisions"),
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(history["revisions"].as_array().unwrap().len(), 3);
    let (status, _, restored) = call(
        &app,
        "POST",
        &format!("{path}/restore"),
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"revision":second["revision"],"from":saved["revision"]}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["html"], "<p>First version</p>");
    assert_ne!(restored["revision"], saved["revision"]);
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{path}/restore"),
            Some(&cookie),
            Some(&csrf),
            serde_json::json!({"revision":second["revision"],"from":saved["revision"]})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, _, trashed) = call(
        &app,
        "POST",
        &format!("{path}/trash"),
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"revision":restored["revision"],"trashed":true}),
    )
    .await;
    assert_eq!(trashed["trashed"], true);
    let root = app.store.lock().unwrap().root.clone();
    assert!(Store::open(root.clone()).unwrap().entries[id].trashed);
    let (_, _, untrashed) = call(
        &app,
        "POST",
        &format!("{path}/trash"),
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"revision":trashed["revision"],"trashed":false}),
    )
    .await;
    assert_eq!(untrashed["trashed"], false);
    assert!(!Store::open(root).unwrap().entries[id].trashed);
    assert_eq!(
        call(
            &app,
            "POST",
            &format!("{path}/trash"),
            Some(&cookie),
            None,
            serde_json::json!({"revision":untrashed["revision"],"trashed":true})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn categories_crud_checks_conflicts_usage_and_persists_catalog() {
    let (_temp, app) = fixture();
    let (cookie, csrf) = authenticated(&app);
    let (_, _, catalog) = call(
        &app,
        "GET",
        "/api/categories",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    let create = serde_json::json!({"revision":catalog["revision"],"name":"New category","slug":"new-category"});
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/categories",
            Some(&cookie),
            Some(&csrf),
            create.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/categories",
            Some(&cookie),
            Some(&csrf),
            create
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, _, catalog) = call(
        &app,
        "GET",
        "/api/categories",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    let rename =
        serde_json::json!({"revision":catalog["revision"],"name":"Renamed","slug":"changed-slug"});
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/categories/new-category",
            Some(&cookie),
            Some(&csrf),
            rename
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/categories/new-category",
            Some(&cookie),
            Some(&csrf),
            serde_json::json!({"revision":catalog["revision"],"name":"Renamed"})
        )
        .await
        .0,
        StatusCode::OK
    );
    let root = app.store.lock().unwrap().root.clone();
    assert!(
        Store::open(root.clone())
            .unwrap()
            .site
            .categories
            .iter()
            .any(|c| c.name == "Renamed")
    );
    let (_, _, catalog) = call(
        &app,
        "GET",
        "/api/categories",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(
        call(
            &app,
            "DELETE",
            "/api/categories/artigos",
            Some(&cookie),
            Some(&csrf),
            serde_json::json!({"revision":catalog["revision"]})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &app,
            "DELETE",
            "/api/categories/new-category",
            Some(&cookie),
            Some(&csrf),
            serde_json::json!({"revision":catalog["revision"]})
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(Store::open(root).unwrap().site.categories.len(), 1);
}
