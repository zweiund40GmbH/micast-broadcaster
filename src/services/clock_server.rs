
pub fn service() -> Result<(), anyhow::Error> {
    Ok(super::informip::inform_clients())
}