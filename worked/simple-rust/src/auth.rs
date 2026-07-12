use crate::db::find_user;

pub fn login(email: &str, password: &str) -> Option<String> {
    find_user(email).map(|_| "token".to_string())
}

pub fn logout(_token: &str) {}
