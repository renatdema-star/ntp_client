use std::io::{self, Error, ErrorKind};
use std::net::UdpSocket;
use std::time::Duration;

const NTP_SERVER: &str = "pool.ntp.org:123";
const NTP_MESSAGE_LENGTH: usize = 48;
const NTP_TO_UNIX_SECONDS: u64 = 2_208_988_800;

fn main() {
    println!("Запуск NTP-клієнта...");

    match fetch_ntp_time() {
        Ok(unix_time) => {
            println!("Успішно! Отримано точний час (Unix Timestamp): {}", unix_time);

            match set_system_time(unix_time) {
                Ok(_) => println!("Системний час успішно синхронізовано!"),
                Err(e) => eprintln!("Помилка при встановленні системного часу (можливо, потрібні права адміністратора): {}", e),
            }
        }
        Err(e) => eprintln!("Помилка під час отримання часу: {}", e),
    }
}

fn fetch_ntp_time() -> io::Result<u64> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    socket.set_write_timeout(Some(Duration::from_secs(5)))?;

    // Запит: LI = 0, VN = 3, Mode = 3 (клієнт)
    let mut request = [0u8; NTP_MESSAGE_LENGTH];
    request[0] = 0b00011011;
    socket.send_to(&request, NTP_SERVER)?;

    let mut response = [0u8; NTP_MESSAGE_LENGTH];
    let (received, _source) = socket.recv_from(&mut response)?;

    if received < NTP_MESSAGE_LENGTH {
        return Err(Error::new(ErrorKind::InvalidData, "Відповідь сервера занадто коротка."));
    }
    if response[0] & 0b0000_0111 != 4 {
        return Err(Error::new(ErrorKind::InvalidData, "Відповідь не від NTP-сервера (режим не дорівнює 4)."));
    }

    // Байти 40-43: секунди Transmit Timestamp
    let transmit_tm = u32::from_be_bytes([
        response[40], response[41], response[42], response[43],
    ]);
    ntp_seconds_to_unix(transmit_tm)
}

fn ntp_seconds_to_unix(ntp_seconds: u32) -> io::Result<u64> {
    if ntp_seconds == 0 {
        return Err(Error::new(ErrorKind::InvalidData, "Отримано некоректний час від сервера."));
    }
    (ntp_seconds as u64)
        .checked_sub(NTP_TO_UNIX_SECONDS)
        .ok_or_else(|| Error::new(ErrorKind::InvalidData, "Час сервера раніше 1970 року."))
}

#[cfg(unix)]
fn set_system_time(unix_seconds: u64) -> io::Result<()> {
    let tv = libc::timeval {
        tv_sec: unix_seconds as libc::time_t,
        tv_usec: 0,
    };
    let res = unsafe { libc::settimeofday(&tv, std::ptr::null()) };
    if res == 0 {
        Ok(())
    } else {
        Err(Error::last_os_error())
    }
}

#[cfg(windows)]
fn set_system_time(unix_seconds: u64) -> io::Result<()> {
    use winapi::um::sysinfoapi::SetSystemTime;
    use winapi::shared::minwindef::FILETIME;
    use winapi::um::minwinbase::SYSTEMTIME;
    use winapi::um::timezoneapi::FileTimeToSystemTime;

    let windows_ticks = (unix_seconds + 11_644_473_600) * 10_000_000;
    let ft = FILETIME {
        dwLowDateTime: (windows_ticks & 0xFFFFFFFF) as u32,
        dwHighDateTime: (windows_ticks >> 32) as u32,
    };
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let res1 = unsafe { FileTimeToSystemTime(&ft, &mut st) };
    if res1 == 0 {
        return Err(Error::last_os_error());
    }
    let res2 = unsafe { SetSystemTime(&st) };
    if res2 != 0 {
        Ok(())
    } else {
        Err(Error::last_os_error())
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_known_date() {
        // 1 січня 2024 року 00:00:00 UTC
        assert_eq!(ntp_seconds_to_unix(3_913_056_000).unwrap(), 1_704_067_200);
    }

    #[test]
    fn epoch_boundary_gives_zero() {
        assert_eq!(ntp_seconds_to_unix(2_208_988_800).unwrap(), 0);
    }

    #[test]
    fn rejects_zero_timestamp() {
        let err = ntp_seconds_to_unix(0).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }

    #[test]
    fn rejects_time_before_1970() {
        assert!(ntp_seconds_to_unix(1_000).is_err());
    }
}
