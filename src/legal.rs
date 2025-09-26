use crate::prelude::*;

#[rocket::get("/legal")]
pub(crate) async fn legal_disclaimer(pool: &State<PgPool>, me: Option<User>, uri: Origin<'_>) -> PageResult {
    page(pool.begin().await?, &me, &uri, PageStyle::default(), "Legal — Hyrule Town Hall", html! {
            p {
                strong : "Information in accordance with Section 5 TMG";
            }

            p {
                : "Christoph Wergen"; br;
                : "ZeldaSpeedRuns"; br;
                : "Am Weyerhof 2"; br;
                : "50226 Frechen"; br;
                : "Germany";
            }

            h2 : "Contact Information";
            p {
                : "Email: "; a(href="mailto:zsrstaff@gmail.com") : "zsrstaff@gmail.com";
            }

            h2 : "Disclaimer";
            h3 : "Accountability for content";
            p {
                : "The site contents of our pages have been created with the utmost care. However, we cannot guarantee their accuracy, completeness or topicality. \
                  According to statutory provisions, we are responsible for our own content on these web pages. Please note that we are not obliged to monitor transmitted or saved third-party information, \
                  nor to investigate circumstances pointing to illegal activity. Our obligations to remove or block illegal content under §§ 8–10 TMG remain unaffected.";
            }

            h3 : "Accountability for links";
            p {
                : "Responsibility for the content of external links lies solely with the operators of the linked pages. No infringements were apparent at the time of linking. \
                  Should any legal infringement become known, we will remove the link immediately.";
            }

            h3 : "Copyright";
            p {
                : "All content on this site is subject to German copyright law. Unless expressly permitted by law, any use, reproduction or processing of works covered by copyright requires the prior consent of the rights holder. \
                  Individual reproductions for private use are permitted. Unauthorized use may violate copyright laws.";
            }

            h2 : "Privacy Policy";
            p {
                : "We process personal data only to the extent necessary for a functional, user-friendly website. “Processing” is defined by Art. 4(1) GDPR and includes collection, storage, use, disclosure, deletion, etc.";
            }
            p : "This policy covers:";
            ol {
                li { 
                    strong : "I. Information about us as controllers of your data";
                };
                li { 
                    strong : "II. The rights of users and data subjects";
                };
                li { 
                    strong : "III. Information about the data processing";
                };
                li { 
                    strong : "IV. Detailed info on processing your personal data";
                };
            }

            h3 : "I. Information about us as controllers of your data";
            p {
                strong : "ZeldaSpeedRuns"; br;
                : "Am Weyerhof 2"; br;
                : "50226 Frechen"; br;
                : "Germany"; br;
                : "Email: "; a(href="mailto:zsrstaff@gmail.com") : "zsrstaff@gmail.com";
            }

            h3 : "II. The rights of users and data subjects";
            ul {
                li : "Right of access (Art. 15 GDPR)";
                li : "Right to rectification (Art. 16 GDPR)";
                li : "Right to erasure or restriction (Arts. 17–18 GDPR)";
                li : "Right to data portability (Art. 20 GDPR)";
                li : "Right to lodge a complaint (Art. 77 GDPR)";
                li : "Right to object to processing (Art. 21 GDPR), including direct marketing";
            }

            h3 : "III. Information about the data processing";
            p : "We delete or block personal data once the processing purpose ceases, unless statutory retention requires otherwise.";

            h3 : "IV. Detailed info on processing your personal data";
            p {
                : "We may use third-party services (e.g., Google Analytics) to analyze site usage. \
                  For details and opt-out see "; a(href="https://policies.google.com/privacy") : "Google Privacy & Terms"; 
                : ".";
            }

            p {
                em : "HTH is not responsible for the actions or failures of third parties.";
            }
    }).await
}
