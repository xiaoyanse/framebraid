//! 无凭据三流地址生成；相机配置和探测结果由调用方保存。
use crate::{Error, Result};

#[derive(Debug, Clone)]
pub struct CameraEndpoint {
    host: String,
    http_port: u16,
    rtsp_port: u16,
    main_channel: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamAddresses {
    pub isapi: String,
    pub rtsp_main: String,
    pub rtsp_sub: String,
}

impl CameraEndpoint {
    /// host 仅接受主机名或 IP，不接受 URL、凭据或路径。
    pub fn new(host: &str, http_port: u16, rtsp_port: u16, main_channel: u16) -> Result<Self> {
        if host.is_empty()
            || host.chars().any(char::is_whitespace)
            || host.contains(['/', '\\', '@', '?', '#', '%'])
            || http_port == 0
            || rtsp_port == 0
            || main_channel == 0
            || main_channel == u16::MAX
        {
            return Err(Error::Invalid("camera.addressInvalid".into()));
        }
        let literal = if let Ok(ip) = host.parse::<std::net::Ipv6Addr>() {
            format!("[{ip}]")
        } else {
            host.to_owned()
        };
        let parsed = reqwest::Url::parse(&format!("http://{literal}:{http_port}/"))
            .map_err(|_| Error::Invalid("camera.addressInvalid".into()))?;
        if parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.port_or_known_default() != Some(http_port)
        {
            return Err(Error::Invalid("camera.addressInvalid".into()));
        }
        Ok(Self {
            host: parsed.host_str().unwrap_or_default().to_owned(),
            http_port,
            rtsp_port,
            main_channel,
        })
    }

    /// 海康通道约定：101 为主码流，102 为同通道子码流。
    pub fn addresses(&self) -> StreamAddresses {
        StreamAddresses {
            isapi: format!(
                "http://{}:{}/ISAPI/Streaming/channels/{}/picture",
                self.host, self.http_port, self.main_channel
            ),
            rtsp_main: format!(
                "rtsp://{}:{}/Streaming/Channels/{}",
                self.host, self.rtsp_port, self.main_channel
            ),
            rtsp_sub: format!(
                "rtsp://{}:{}/Streaming/Channels/{}",
                self.host,
                self.rtsp_port,
                self.main_channel + 1
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_all_three_addresses_with_explicit_ports() {
        let addresses = CameraEndpoint::new("127.0.0.1", 8080, 8554, 201)
            .unwrap()
            .addresses();
        assert_eq!(
            addresses.isapi,
            "http://127.0.0.1:8080/ISAPI/Streaming/channels/201/picture"
        );
        assert_eq!(
            addresses.rtsp_main,
            "rtsp://127.0.0.1:8554/Streaming/Channels/201"
        );
        assert_eq!(
            addresses.rtsp_sub,
            "rtsp://127.0.0.1:8554/Streaming/Channels/202"
        );
    }

    #[test]
    fn supports_ipv6_and_dns_without_embedded_credentials() {
        for host in ["::1", "[::1]"] {
            let addresses = CameraEndpoint::new(host, 80, 554, 101).unwrap().addresses();
            assert_eq!(
                addresses.rtsp_main,
                "rtsp://[::1]:554/Streaming/Channels/101"
            );
        }
        let addresses = CameraEndpoint::new("camera.example", 80, 554, 101)
            .unwrap()
            .addresses();
        assert!(addresses.isapi.starts_with("http://camera.example:80/"));
    }

    #[test]
    fn rejects_credential_url_injection_and_invalid_channels() {
        for host in [
            "",
            "user:password@camera",
            "http://camera",
            "camera/path",
            "camera:9000",
            " camera",
            "camera?x",
        ] {
            assert!(CameraEndpoint::new(host, 80, 554, 101).is_err());
        }
        assert!(CameraEndpoint::new("localhost", 0, 554, 101).is_err());
        assert!(CameraEndpoint::new("localhost", 80, 554, 0).is_err());
        assert!(CameraEndpoint::new("localhost", 80, 554, u16::MAX).is_err());
    }
}
