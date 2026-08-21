use crate::error::AppError;
use tauri::AppHandle;

#[cfg(target_os = "android")]
use serde::{Deserialize, Serialize};
#[cfg(target_os = "android")]
use tauri::{
    plugin::{Builder, PluginHandle, TauriPlugin},
    Manager, Runtime,
};

#[cfg(target_os = "android")]
const ANDROID_PLUGIN_IDENTIFIER: &str = "cn.surkaa.pad.location";

fn validate_coordinates(latitude: f64, longitude: f64) -> Result<(), AppError> {
    if !latitude.is_finite() || !(-90.0..=90.0).contains(&latitude) {
        return Err(AppError {
            error_type: "invalid_location".into(),
            message: "纬度必须位于 -90 到 90 度之间".into(),
        });
    }
    if !longitude.is_finite() || !(-180.0..=180.0).contains(&longitude) {
        return Err(AppError {
            error_type: "invalid_location".into(),
            message: "经度必须位于 -180 到 180 度之间".into(),
        });
    }
    Ok(())
}

/// 使用 Android 系统逆地理编码器尝试取得可编辑的地点名称。
///
/// Windows 不主动获取位置或解析地点名称，因此返回 `None`。
#[tauri::command]
#[specta::specta]
pub async fn cmd_reverse_geocode(
    app: AppHandle,
    latitude: f64,
    longitude: f64,
) -> Result<Option<String>, AppError> {
    validate_coordinates(latitude, longitude)?;

    #[cfg(target_os = "android")]
    {
        return app
            .android_location_name()
            .reverse_geocode(latitude, longitude)
            .map(|response| response.place_name)
            .map_err(|error| AppError {
                error_type: "reverse_geocode".into(),
                message: error.to_string(),
            });
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(None)
    }
}

#[cfg(target_os = "android")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReverseGeocodeRequest {
    latitude: f64,
    longitude: f64,
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReverseGeocodeResponse {
    #[serde(default)]
    place_name: Option<String>,
}

#[cfg(target_os = "android")]
struct AndroidLocationName<R: Runtime>(PluginHandle<R>);

#[cfg(target_os = "android")]
impl<R: Runtime> AndroidLocationName<R> {
    fn reverse_geocode(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<ReverseGeocodeResponse, tauri::plugin::mobile::PluginInvokeError> {
        self.0.run_mobile_plugin(
            "reverseGeocode",
            ReverseGeocodeRequest {
                latitude,
                longitude,
            },
        )
    }
}

#[cfg(target_os = "android")]
trait AndroidLocationNameExt<R: Runtime> {
    fn android_location_name(&self) -> &AndroidLocationName<R>;
}

#[cfg(target_os = "android")]
impl<R: Runtime, T: Manager<R>> AndroidLocationNameExt<R> for T {
    fn android_location_name(&self) -> &AndroidLocationName<R> {
        self.state::<AndroidLocationName<R>>().inner()
    }
}

#[cfg(target_os = "android")]
pub fn init_android_plugin<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("location-name")
        .setup(|app, api| {
            let handle =
                api.register_android_plugin(ANDROID_PLUGIN_IDENTIFIER, "LocationNamePlugin")?;
            app.manage(AndroidLocationName(handle));
            Ok(())
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::validate_coordinates;

    #[test]
    fn validates_coordinate_boundaries() {
        assert!(validate_coordinates(-90.0, -180.0).is_ok());
        assert!(validate_coordinates(90.0, 180.0).is_ok());
        assert!(validate_coordinates(90.000_001, 0.0).is_err());
        assert!(validate_coordinates(0.0, -180.000_001).is_err());
        assert!(validate_coordinates(f64::NAN, 0.0).is_err());
    }
}
