const EARTH_RADIUS_KM: f64 = 6_371.0;

#[must_use]
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let latitude_delta = (lat2 - lat1).to_radians();
    let longitude_delta = (lon2 - lon1).to_radians();
    let first_latitude = lat1.to_radians();
    let second_latitude = lat2.to_radians();
    let haversine = (latitude_delta / 2.0).sin().powi(2)
        + first_latitude.cos() * second_latitude.cos() * (longitude_delta / 2.0).sin().powi(2);
    let central_angle = 2.0 * haversine.sqrt().atan2((1.0 - haversine).max(0.0).sqrt());
    EARTH_RADIUS_KM * central_angle
}
