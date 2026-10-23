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
async fn library_upload_order_is_persistent_precedes_pagination_and_ignores_caption_edits() {
    let (temp, app) = fixture();
    let root = temp.path().join("content");
    for i in 0..50 {
        fs::write(root.join(format!("media/uploads/a-old-{i}.png")), b"image").unwrap();
    }
    for name in ["zzz-first.png", "yyy-second.png"] {
        fs::write(root.join("media/uploads").join(name), b"image").unwrap();
        library::uploaded(&root, &format!("/uploads/{name}"), name).unwrap();
    }
    let (cookie, csrf) = authenticated(&app);
    let (_, _, first) = call(
        &app,
        "GET",
        "/api/library",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(first["total"], 52);
    assert_eq!(first["images"].as_array().unwrap().len(), 48);
    assert_eq!(first["images"][0]["path"], "/uploads/yyy-second.png");
    assert_eq!(first["images"][1]["path"], "/uploads/zzz-first.png");
    let timestamp = first["images"][1]["uploaded_at"].clone();
    let edit = serde_json::json!({"path":"/uploads/zzz-first.png","revision":first["revision"],"alt":"Description","caption":"Saved caption","deleted":false});
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/library",
            Some(&cookie),
            Some(&csrf),
            edit
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, _, second) = call(
        &app,
        "GET",
        "/api/library?offset=48",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(second["images"].as_array().unwrap().len(), 4);
    assert!(
        second["images"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| !i["path"].as_str().unwrap().contains("first")
                && !i["path"].as_str().unwrap().contains("second"))
    );
    drop(app);
    let app = make_app(
        root.clone(),
        &temp.path().join("state"),
        "http://localhost:8781".into(),
        "localhost".into(),
    )
    .unwrap();
    let (_, _, reopened) = call(
        &app,
        "GET",
        "/api/library",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(reopened["images"][0]["path"], "/uploads/yyy-second.png");
    assert_eq!(reopened["images"][1]["uploaded_at"], timestamp);
    library::uploaded(&root, "/uploads/zzz-first.png", "zzz-first.png").unwrap();
    let (_, _, uploaded_again) = call(
        &app,
        "GET",
        "/api/library",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(uploaded_again["total"], 52);
    assert_eq!(
        uploaded_again["images"][0]["path"],
        "/uploads/zzz-first.png"
    );
    assert_eq!(uploaded_again["images"][0]["caption"], "Saved caption");
}

#[tokio::test]
async fn empty_trash_requires_confirmation_rejects_races_and_survives_restart() {
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
    let (_, _, trashed) = call(
        &app,
        "POST",
        "/api/articles/wp-1/trash",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"revision":original["revision"],"trashed":true}),
    )
    .await;
    let (_, _, summary) = call(
        &app,
        "GET",
        "/api/trash",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(summary["items"].as_array().unwrap().len(), 1);
    let mut payload =
        serde_json::json!({"revision":summary["revision"],"confirmation":"ESVAZIAR LIXEIRA"});
    assert_eq!(
        call(&app, "POST", "/api/trash", None, None, payload.clone())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/trash",
            Some(&cookie),
            None,
            payload.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    payload["confirmation"] = serde_json::json!("yes");
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/trash",
            Some(&cookie),
            Some(&csrf),
            payload.clone()
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    payload["confirmation"] = serde_json::json!("ESVAZIAR LIXEIRA");
    let (_, _, restored) = call(
        &app,
        "POST",
        "/api/articles/wp-1/trash",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"revision":trashed["revision"],"trashed":false}),
    )
    .await;
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/trash",
            Some(&cookie),
            Some(&csrf),
            payload.clone()
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/articles/wp-1/trash",
            Some(&cookie),
            Some(&csrf),
            serde_json::json!({"revision":restored["revision"],"trashed":true})
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, _, summary) = call(
        &app,
        "GET",
        "/api/trash",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    payload["revision"] = summary["revision"].clone();
    // Another active draft must not be removed.
    let (_, _, survivor) = call(
        &app,
        "POST",
        "/api/articles",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({}),
    )
    .await;
    fs::write(
        temp.path().join("content/media/uploads/shared.png"),
        b"image",
    )
    .unwrap();
    let (status, _, result) = call(
        &app,
        "POST",
        "/api/trash",
        Some(&cookie),
        Some(&csrf),
        payload,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(result["deleted"], 1);
    assert_eq!(result["cleanup_pending"], 0);
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/articles/wp-1",
            Some(&cookie),
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert!(!temp.path().join("content/editorial/wp-1").exists());
    assert!(temp.path().join("content/articles/wp-1/body.html").exists());
    assert!(
        temp.path()
            .join("content/media/uploads/shared.png")
            .exists()
    );
    let reopened = Store::open(temp.path().join("content")).unwrap();
    assert!(!reopened.entries.contains_key("wp-1"));
    assert!(
        reopened
            .entries
            .contains_key(survivor["metadata"]["id"].as_str().unwrap())
    );
}

#[tokio::test]
async fn image_library_is_private_versioned_and_preserves_existing_urls() {
    let (temp, app) = fixture();
    let image = "/uploads/library.png";
    fs::write(
        temp.path().join("content/media/uploads/library.png"),
        b"existing-passive-image",
    )
    .unwrap();
    let (cookie, csrf) = authenticated(&app);
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/library",
            None,
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    let (_, _, catalog) = call(
        &app,
        "GET",
        "/api/library",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(catalog["total"], 1);
    let update = serde_json::json!({"path":image,"revision":catalog["revision"],"alt":"Descrição <segura>","caption":"Legenda & poesia","deleted":false});
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/library",
            Some(&cookie),
            None,
            update.clone()
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/library",
            Some(&cookie),
            Some(&csrf),
            update.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/library",
            Some(&cookie),
            Some(&csrf),
            update
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (_, _, catalog) = call(
        &app,
        "GET",
        "/api/library?q=descricao",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(catalog["images"][0]["alt"], "Descrição <segura>");
    let mut update = serde_json::json!({"path":image,"revision":catalog["revision"],"alt":"Descrição <segura>","caption":"Legenda & poesia","deleted":true});
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/library",
            Some(&cookie),
            Some(&csrf),
            update.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    let (_, _, active) = call(
        &app,
        "GET",
        "/api/library",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(active["total"], 0);
    let (_, _, removed) = call(
        &app,
        "GET",
        "/api/library?deleted=true",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(removed["total"], 1);
    assert_eq!(
        fs::read(temp.path().join("content/media/uploads/library.png")).unwrap(),
        b"existing-passive-image"
    );
    update["revision"] = removed["revision"].clone();
    update["deleted"] = serde_json::json!(false);
    assert_eq!(
        call(
            &app,
            "PUT",
            "/api/library",
            Some(&cookie),
            Some(&csrf),
            update
        )
        .await
        .0,
        StatusCode::OK
    );
    assert!(Store::open(temp.path().join("content")).is_ok());
    fs::write(
        temp.path().join("content/media/uploads/ação #1?.png"),
        b"passive-image",
    )
    .unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/media/uploads/a%C3%A7%C3%A3o%20%231%3F.png",
            Some(&cookie),
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::OK
    );
}

#[tokio::test]
async fn production_identity_and_duplicate_featured_preview() {
    let (temp, mut app) = fixture();
    let mutable = Arc::get_mut(&mut app).unwrap();
    mutable.environment = "production".into();
    mutable.environment_label = "Produção".into();
    let (_, _, status) = call(
        &app,
        "GET",
        "/api/status",
        None,
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status["staging"], false);
    assert_eq!(status["environment"], "production");
    let (cookie, csrf) = authenticated(&app);
    let (_, _, session) = call(
        &app,
        "GET",
        "/api/session",
        Some(&cookie),
        None,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(session["environment_label"], "Produção");
    fs::write(
        temp.path().join("content/media/uploads/one.png"),
        b"passive-image",
    )
    .unwrap();
    let payload = serde_json::json!({"title":"One image","html":"<figure><img src='/uploads/one.png' alt='Body description'><figcaption>Body caption</figcaption></figure>","featured_image":"/uploads/one.png","featured_image_alt":"Old description","featured_image_caption":"Old caption"});
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
    let html = preview["html"].as_str().unwrap();
    assert_eq!(html.matches("<img ").count(), 1);
    assert!(html.contains("Body caption"));
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
#[ignore = "requires root; disposable recovery fixture only"]
async fn offline_recovery_revokes_old_access_and_enrolls_replacement() {
    assert_eq!(unsafe { libc::geteuid() }, 0);
    let (temp, app) = fixture();
    let origin = url::Url::parse("http://localhost:8781").unwrap();
    let mut old_key = WebauthnAuthenticator::new(SoftPasskey::new(true));
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
    let credential = old_key
        .do_registration(
            origin.clone(),
            serde_json::from_value(flow["options"].clone()).unwrap(),
        )
        .unwrap();
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/auth/register/finish",
        Some(&cookie),
        None,
        serde_json::json!({"id":flow["id"],"credential":credential,"name":"Disposable old key"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let old_session = session_cookie(&headers, SESSION_COOKIE);
    let (_, headers, pending) = call(
        &app,
        "POST",
        "/api/auth/login/start",
        None,
        None,
        serde_json::json!({}),
    )
    .await;
    let old_flow_cookie = session_cookie(&headers, FLOW_COOKIE);
    let old_assertion = old_key
        .do_authentication(
            origin.clone(),
            serde_json::from_value(pending["options"].clone()).unwrap(),
        )
        .unwrap();

    let secret = temp.path().join("offline-secret");
    fs::write(&secret, "offline-test-secret").unwrap();
    let state = temp.path().join("state");
    let outputs = temp.path().join("rotated-secrets");
    recover(&state, &secret, &outputs).unwrap();
    drop(app); // Recovery requires a restart: discard all in-memory challenges.
    let app = make_app(
        temp.path().join("content"),
        &state,
        "http://localhost:8781".into(),
        "localhost".into(),
    )
    .unwrap();
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/articles",
            Some(&old_session),
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/auth/login/finish",
            Some(&old_flow_cookie),
            None,
            serde_json::json!({"id":pending["id"],"credential":old_assertion})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert!(recover(&state, &secret, &temp.path().join("replay")).is_err());
    let bootstrap = fs::read_to_string(outputs.join("bootstrap-token.txt")).unwrap();
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
            serde_json::json!({"bootstrap":bootstrap})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    app.db
        .lock()
        .unwrap()
        .execute("UPDATE bootstrap SET expires_at=?1", [now() + 600])
        .unwrap();

    let mut replacement = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let (status, headers, flow) = call(
        &app,
        "POST",
        "/api/auth/register/start",
        None,
        None,
        serde_json::json!({"bootstrap":bootstrap}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let cookie = session_cookie(&headers, FLOW_COOKIE);
    let credential = replacement
        .do_registration(
            origin.clone(),
            serde_json::from_value(flow["options"].clone()).unwrap(),
        )
        .unwrap();
    let finish = serde_json::json!({"id":flow["id"],"credential":credential,"name":"Disposable replacement"});
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/auth/register/finish",
            Some(&cookie),
            None,
            finish.clone()
        )
        .await
        .0,
        StatusCode::OK
    );
    assert_eq!(
        call(
            &app,
            "POST",
            "/api/auth/register/finish",
            Some(&cookie),
            None,
            finish
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.db
            .lock()
            .unwrap()
            .query_row("SELECT count(*) FROM bootstrap", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );

    let (_, headers, flow) = call(
        &app,
        "POST",
        "/api/auth/login/start",
        None,
        None,
        serde_json::json!({}),
    )
    .await;
    let cookie = session_cookie(&headers, FLOW_COOKIE);
    assert!(
        old_key
            .do_authentication(
                origin.clone(),
                serde_json::from_value(flow["options"].clone()).unwrap()
            )
            .is_err()
    );
    let credential = replacement
        .do_authentication(
            origin,
            serde_json::from_value(flow["options"].clone()).unwrap(),
        )
        .unwrap();
    let (status, headers, _) = call(
        &app,
        "POST",
        "/api/auth/login/finish",
        Some(&cookie),
        None,
        serde_json::json!({"id":flow["id"],"credential":credential}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session = session_cookie(&headers, SESSION_COOKIE);
    assert_eq!(
        call(
            &app,
            "GET",
            "/api/articles",
            Some(&session),
            None,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::OK
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

#[tokio::test]
async fn automatic_permalinks_normalize_check_collisions_and_recheck_on_save() {
    let (_temp, app) = fixture();
    let (cookie, csrf) = authenticated(&app);
    let (status, _, _) = call(
        &app,
        "POST",
        "/api/articles/wp-1/permalink",
        None,
        None,
        serde_json::json!({"title":"Teste"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, _, _) = call(
        &app,
        "POST",
        "/api/articles/wp-1/permalink",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"title":"Teste"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST); // Published URLs stay immutable.
    let (_, _, first) = call(
        &app,
        "POST",
        "/api/articles",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({}),
    )
    .await;
    let (_, _, second) = call(
        &app,
        "POST",
        "/api/articles",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({}),
    )
    .await;
    let id = first["metadata"]["id"].as_str().unwrap();
    let route = format!("/api/articles/{id}/permalink");
    let (status, _, suggestion) = call(
        &app,
        "POST",
        &route,
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"title":"Téste de Artigo!!!"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(suggestion["permalink"], "/artigos/teste-de-artigo/");
    let (_, _, duplicate) = call(
        &app,
        "POST",
        &route,
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"title":"Original"}),
    )
    .await;
    let duplicate_path = duplicate["permalink"].as_str().unwrap();
    assert!(duplicate_path.starts_with("/artigos/original-"));
    assert_eq!(duplicate_path.len(), "/artigos/original-".len() + 7);
    let (_, _, repeated) = call(
        &app,
        "POST",
        &route,
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({"title":"Original","current":duplicate_path}),
    )
    .await;
    assert_eq!(duplicate, repeated);
    let mut saved_paths = Vec::new();
    for document in [&first, &second] {
        let payload = serde_json::json!({"revision":document["revision"],"title":"Teste de Artigo","html":"<p>Body</p>","permalink":"/artigos/teste-de-artigo/","auto_permalink":true,"categories":["artigos"],"featured_image":null,"featured_image_alt":"","featured_image_caption":"","published_local":"2026-10-23T15:00:00"});
        let (status, _, saved) = call(
            &app,
            "PUT",
            &format!(
                "/api/articles/{}",
                document["metadata"]["id"].as_str().unwrap()
            ),
            Some(&cookie),
            Some(&csrf),
            payload,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{saved}");
        saved_paths.push(saved["metadata"]["permalink"].as_str().unwrap().to_string());
    }
    assert_eq!(saved_paths[0], "/artigos/teste-de-artigo/");
    assert_ne!(saved_paths[0], saved_paths[1]);
    assert!(saved_paths[1].starts_with("/artigos/teste-de-artigo-"));
    let store = app.store.lock().unwrap();
    let doc = store.entries.get(id).unwrap();
    assert_eq!(
        permalinks::available(&store, doc, "Teste de Artigo", Some(&saved_paths[0])).unwrap(),
        saved_paths[0]
    );
}

#[tokio::test]
async fn automatic_permalinks_reserve_redirects_attachments_trash_and_published_history() {
    let (_temp, app) = fixture();
    let (cookie, csrf) = authenticated(&app);
    let (_, _, draft) = call(
        &app,
        "POST",
        "/api/articles",
        Some(&cookie),
        Some(&csrf),
        serde_json::json!({}),
    )
    .await;
    let id = draft["metadata"]["id"].as_str().unwrap();
    let (status, _, _) = call(
        &app,
        "POST",
        &format!("/api/articles/{id}/permalink"),
        Some(&cookie),
        None,
        serde_json::json!({"title":"Blocked"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let mut store = app.store.lock().unwrap();
    store
        .site
        .redirects
        .insert("/artigos/redirect/".into(), "/artigos/original/".into());
    store.site.observed_paths.push("/artigos/observed/".into());
    store.entries.get_mut("wp-1").unwrap().trashed = true;
    write_json(&store.root.join("attachments.json"),&serde_json::json!([{"id":"2","parent":"1","title":"Attachment","path":"/uploads/a.jpg","alt":"","caption":"","description":"","permalink":"/artigos/attachment/"}])).unwrap();
    let old = store.root.join("editorial/purged-entry");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("published-path"), "/artigos/history/").unwrap();
    let document = store.entries.get(id).unwrap();
    for title in ["redirect", "observed", "attachment", "original", "history"] {
        let suggested = permalinks::available(&store, document, title, None).unwrap();
        assert!(
            suggested.starts_with(&format!("/artigos/{title}-")),
            "{suggested}"
        );
    }
    let mut page = document.clone();
    page.metadata.kind = "page".into();
    assert_eq!(
        permalinks::available(&store, &page, "Minha Página", None).unwrap(),
        "/minha-pagina/"
    );
    assert!(
        permalinks::available(&store, &page, "Feed", None)
            .unwrap()
            .starts_with("/feed-")
    );
}
