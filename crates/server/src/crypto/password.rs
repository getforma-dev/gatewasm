use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed_hash = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

pub fn validate_password_complexity(password: &str) -> Result<(), &'static str> {
    if password.len() < 8 {
        return Err("Password must be at least 8 characters");
    }
    if !password.chars().any(|c| c.is_uppercase()) {
        return Err("Password must contain at least 1 uppercase letter");
    }
    if !password.chars().any(|c| c.is_lowercase()) {
        return Err("Password must contain at least 1 lowercase letter");
    }
    if !password.chars().any(|c| c.is_ascii_digit()) {
        return Err("Password must contain at least 1 digit");
    }
    let lower = password.to_lowercase();
    if COMMON_PASSWORDS.contains(&lower.as_str()) {
        return Err("This password is too common — please choose a more unique password");
    }
    Ok(())
}

const COMMON_PASSWORDS: &[&str] = &[
    "password",
    "12345678",
    "123456789",
    "1234567890",
    "qwerty12",
    "password1",
    "password123",
    "iloveyou",
    "sunshine1",
    "princess1",
    "football1",
    "charlie1",
    "access14",
    "trustno1",
    "dragon12",
    "master12",
    "monkey12",
    "shadow12",
    "michael1",
    "qwerty123",
    "abc12345",
    "abcdefgh",
    "abcd1234",
    "letmein1",
    "welcome1",
    "admin123",
    "login123",
    "passw0rd",
    "starwars1",
    "batman123",
    "superman1",
    "qazwsx12",
    "mustang1",
    "jennifer1",
    "jordan23",
    "hunter12",
    "ranger12",
    "buster12",
    "thomas12",
    "robert12",
    "soccer12",
    "hockey12",
    "killer12",
    "george12",
    "andrew12",
    "charlie12",
    "daniel12",
    "computer1",
    "internet1",
    "summer12",
    "whatever1",
    "changeme1",
    "password12",
    "baseball1",
    "matthew12",
    "corvette1",
    "mercedes1",
    "1qaz2wsx",
    "qwer1234",
    "asdf1234",
    "zxcv1234",
    "pass1234",
    "test1234",
    "user1234",
    "temp1234",
    "default1",
    "nothing1",
    "security1",
    "1q2w3e4r",
    "q1w2e3r4",
    "asdfghjk",
    "zxcvbnm1",
    "p@ssw0rd",
    "p@ssword",
    "pa$$w0rd",
    "password!",
    "12345678!",
    "qwerty!@#",
    "letmein!",
    "welcome!",
    "abc123456",
    "123456abc",
    "1234abcd",
    "abcd12345",
    "test12345",
];
