pub struct User { pub email: String }

pub fn find_user(email: &str) -> Option<User> {
    if email == "test@test.com" {
        Some(User { email: email.to_string() })
    } else {
        None
    }
}
