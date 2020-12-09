use std::collections::HashMap;

use kuchiki::*;
use kuchiki::traits::*;

use html_diff::{get_differences, Difference};

pub struct MoodleContext {
    client: reqwest::Client
}

impl MoodleContext {
    pub async fn login(auth: &MoodleAuthConf) -> std::result::Result<Self, String> {
        match auth {
            MoodleAuthConf::ShibbolethUser(user, pass) => {
                let client = reqwest::ClientBuilder::new()
                    .cookie_store(true)
                    .build().unwrap();

                let resp = client.get("https://www.moodle.tum.de/Shibboleth.sso/Login?providerId=https%3A%2F%2Ftumidp.lrz.de%2Fidp%2Fshibboleth&target=https%3A%2F%2Fwww.moodle.tum.de%2Fauth%2Fshibboleth%2Findex.php")
                    .header("Referer", "https://www.moodle.tum.de/")
                    .send().await.or(Err(format!("Couldn't open login page")))?;
                let text = resp.text().await.or(Err(format!("Couldn't get login page text")))?;

                let url = format!("https://login.tum.de{}", text.split("form action=\"").collect::<Vec<_>>()[1].split("\"").collect::<Vec<_>>()[0]);

                let mut form = HashMap::new();
                form.insert("j_username", user.as_str());
                form.insert("j_password", pass.as_str());
                form.insert("donotcache", "1");
                form.insert("_eventId_proceed", "");
                let resp = client.post(&url)
                    .form(&form)
                    .send().await.or(Err(format!("Couldn't send login form to {}", url)))?;
                let text = resp.text().await.or(Err(format!("Couldn't get login form text")))?;

                let relay_state = text.split("name=\"RelayState\" value=\"cookie&#x3a;").collect::<Vec<_>>()[1].split("\"").collect::<Vec<_>>()[0];
                let relay_state = format!("cookie:{}", relay_state);
                let saml_resp = text.split("name=\"SAMLResponse\" value=\"").collect::<Vec<_>>()[1].split("\"").collect::<Vec<_>>()[0].to_string();

                let mut form = HashMap::new();
                form.insert("RelayState", relay_state);
                form.insert("SAMLResponse", saml_resp);
                client.post("https://www.moodle.tum.de/Shibboleth.sso/SAML2/POST")
                    .form(&form)
                    .send().await.or(Err(format!("Couldn't send token form")))?;

                Ok(Self {
                    client
                })
            }
        }
    }

    pub async fn get(&self, id: u32) -> Option<MoodleCourseData> {
        let url = format!("https://www.moodle.tum.de/course/view.php?id={}", id);
        let resp = self.client.get(&url).send().await.unwrap();
        if resp.status() != 200 {
            return None;
        }
        let text = resp.text().await.unwrap();
        let html = parse_html().one(text);

        let mut content = String::new();
        let mut name = String::new();

        for element in html.descendants().elements() {
            let id_attr = element.attributes.borrow().get("id").unwrap_or("").to_string();

            match &*element.name.local {
                "div" if id_attr == "page-content" => {
                    let mut content_buf: Vec<u8> = Vec::new();
                    element.as_node().serialize(&mut content_buf).unwrap();
                    content = String::from_utf8(content_buf).unwrap();
                },
                "h1" => name = element.text_contents(),
                _ => ()
            }
        }

        if &name != "" {
            Some(MoodleCourseData {
                id,
                name,
                url,
                content
            })
        } else {
            None
        }
    }

    pub async fn update(&self, origin: &mut MoodleCourseData) -> Option<String> {
        if let Some(target) = self.get(origin.id()).await {
            if target.content() != origin.content() {
                let diff = origin.user_diff(&target);
                origin.content = target.content;
                diff
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct MoodleCourseData {
    id: u32,
    name: String,
    url: String,
    content: String
}

impl MoodleCourseData {
    #[allow(dead_code)]
    pub fn user_diff(&self, other: &MoodleCourseData) -> Option<String> {
        let change = get_differences(&self.content, &other.content);

        let mut summary = String::new();

        for c in change {
            match c {
                Difference::NotPresent{ opposite_elem, .. } => {
                    if let Some(e) = opposite_elem {
                        let mut content_name = String::new();
                        let mut content_type = String::new();

                        let html = parse_html().one(e.element_content.clone());
                        for e in html.descendants().elements() {
                            let class_attr = e.attributes.borrow().get("class").unwrap_or("").to_string();

                            match &*e.name.local {
                                "span" if class_attr == "instancename" => content_name = e.text_contents(),
                                "span" if class_attr == "accesshide " => content_type = e.text_contents(),
                                _ => ()
                            }
                        }

                        if content_name != "" && content_type != "" {
                            summary.push_str(&format!("New \"{}\" uploaded: \"{}\"\n", &content_type[1..], &content_name[0..(content_name.len() - content_type.len())]));

                        } else {
                            println!("Unrecognised change in course {}:\n{}\n-----", self.id, e.element_content);
                        }
                    }
                },
                _ => ()
            }
        }
        
        if summary != "" {
            Some(summary)
        } else {
            None
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn url(&self) -> &str {
        &self.url
    }
 
    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Clone, Debug)]
pub enum MoodleAuthConf {
    ShibbolethUser(String, String)
}

#[cfg(test)]
use std::fs::read_to_string;

#[test]
fn test_moodle_course_diff() {
    let origin = read_to_string("tests/origin.html").expect("Test origin file missing");
    let target = read_to_string("tests/target.html").expect("Test target file missing");

    let origin = MoodleCourseData {
        id: 0,
        name: "Test".to_string(),
        url: "https://example.com".to_string(),
        content: origin
    };
    let target = MoodleCourseData {
        id: 0,
        name: "Test".to_string(),
        url: "https://example.com".to_string(),
        content: target
    };

    let diff = origin.user_diff(&target).expect("Test files are identical");

    assert_eq!(diff, "New \"Datei\" uploaded: \"NEW CONTENT!\"\nNew \"Textseite\" uploaded: \"MORE CONTENT!\"\n");
}
