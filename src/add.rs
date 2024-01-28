use actix_web::{post, web::Form, web::Path, HttpResponse, Responder};

use url::ParseError;
use url::Url;

use crate::http_error;
use crate::Data;

#[derive(PartialEq, Debug)]
struct NotUniqueError;

fn gen_strid(length: usize) -> String {
    use rand::Rng;
    const CHARSET: [char; 36] = [
        'q', 'w', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p', 'a', 's', 'd', 'f',
        'g', 'h', 'j', 'k', 'l', 'z', 'x', 'c', 'v', 'b', 'n', 'm', '1', '2',
        '3', '4', '5', '6', '7', '8', '9', '0',
    ];

    let mut rng = rand::thread_rng();

    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            CHARSET[idx] as char
        })
        .collect()
}

async fn insert_url(
    data: &Data,
    strid: &str,
    url: &Url,
) -> Result<(String, i32), NotUniqueError> {
    let db = data.client.lock().await;

    let existing_link = db
        .query("SELECT url, id FROM Links WHERE strid = $1", &[&strid])
        .await
        .unwrap();

    let is_http = url.scheme() == "http" || url.scheme() == "https";
    let url = url.as_str();

    let link_exists = !existing_link.is_empty();
    if link_exists {
        let existing_url: String = existing_link[0].get("url");
        if existing_url == url {
            let numid: i32 = existing_link[0].get("id");
            return Ok((String::from(strid), numid));
        } else {
            return Err(NotUniqueError);
        }
    }

    let numid: i32 = db
        .query(
            "INSERT INTO Links (strid, url, is_http)
             VALUES ($1, $2, $3) RETURNING id",
            &[&strid, &url, &is_http],
        )
        .await
        .unwrap()[0]
        .get("id");

    db.execute("commit", &[]).await.unwrap();

    Ok((String::from(strid), numid))
}

async fn parse_url(urlstr: String) -> Result<Url, ParseError> {
    let url = Url::parse(&urlstr);

    if url == Err(ParseError::RelativeUrlWithoutBase) {
        Url::parse(&format!("http://{}", &urlstr))
    } else {
        url
    }
}

#[derive(serde::Deserialize)]
struct Link {
    link: String,
}

#[post("api/add/{strid}")]
async fn with_strid(
    data: Data,
    form: Form<Link>,
    path: Path<String>,
) -> impl Responder {
    let strid = path.into_inner();
    let url = form.link.clone();

    let url = parse_url(url).await.unwrap();

    let response = insert_url(&data, &strid.to_lowercase(), &url).await;
    if response == Err(NotUniqueError) {
        return http_error!(CONFLICT);
    }

    let (_, numid) = response.unwrap();

    HttpResponse::Ok().body(format!("{} {}", numid, strid))
}

#[post("/api/add")]
async fn add(data: Data, form: Form<Link>) -> impl Responder {
    let mut strid_length = data.strid_length.lock().await;

    let url = form.link.clone();

    let url = parse_url(url).await.unwrap();

    let mut response = Err(NotUniqueError);
    while response == Err(NotUniqueError) {
        response = insert_url(&data, &gen_strid(*strid_length), &url).await;
    }

    let (strid, numid) = response.unwrap();
    if numid.ilog(36) + 1 != *strid_length as u32 {
        *strid_length = (numid.ilog(36) + 1) as usize;
    }

    HttpResponse::Ok().body(format!("{} {}", numid, strid))
}
