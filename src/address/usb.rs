use std::{
    fmt::{Display, Write},
    num::ParseIntError,
    str::FromStr,
};

use thiserror::Error;

/// Represents a USB VISA address
#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct UsbAddress {
    /// The USB
    board: Option<u32>,
    /// The USB manufacturer ID. Always hex in UI.
    manufactuer_id: u16,
    /// The USB model code. Always hex in the UI.
    model_code: u16,
    /// Serial number. Not actually a number, and a string. For UI purposes only.
    serial_number: String,
    /// Optional interface number. If None, then lowest number is used.
    interface_number: Option<u16>,
    /// USB INSTR lets the controller interact with the device associated with the resource.
    instr: bool,
}

impl UsbAddress {
    /// Creates a new UsbResource from an address.
    /// Panics on failure. See Self::try_new for a Result
    pub fn new(addr: &str) -> UsbAddress {
        UsbAddress::from_str(addr).unwrap()
    }

    /// Failable creates a new UsbResource from an address.
    pub fn try_new(addr: &str) -> Result<Self, UsbParseError> {
        UsbAddress::from_str(addr)
    }
}

/// Errors that can return from USB address parsing.
#[derive(Error, Debug)]
pub enum UsbParseError {
    /// When the given address does not have the USB prefix.
    #[error("Expected \"USB\" at address start, found {0:?}")]
    NotUSB(String),

    /// When parsing an integer fails.
    #[error("Found {found:?} instead of a number at position {start:?} to {end:?} of \n{addr:?}")]
    NumParseError {
        found: String,
        addr: String,
        start: usize,
        end: usize,
        #[source]
        source: ParseIntError,
    },

    /// When a field that is supposed to be hexidecimal is not properly formatted.
    #[error("Invalid hexidecimal number: {found:?} at position {start:?} to {end:?} in\n {addr:?}\nNumber must start with '0x'")]
    NotHex {
        found: String,
        addr: String,
        start: usize,
        end: usize,
    },

    /// When an address is detected to be incomplete
    #[error("{0:?} is an incomplete address missing: {1}")]
    IncompleteAddress(String, String),

    /// When an address indicates that is has an "INSTR" suffix, but is malformed.
    #[error("In address \"INSTR\" was indicated but instead {found:?} was found at {start:?} to {end:?} of\n {addr:?}")]
    NotInstr {
        found: String,
        addr: String,
        start: usize,
        end: usize,
    },
}

/// State of the USB address parser state-machine
enum UsbParserState {
    /// Required, the initial state
    Usb,

    /// Optional, always transition to second.
    Board,

    /// Required, always transition to second.
    ManufactuerId,

    /// Required, always transition to third.
    ModelCode,

    /// Required, always transition to fourth.
    SerialNumber,

    /// Optional, may trasition to fourth or never be transitioned to is address ends.
    USBInterface,

    /// Optional, may transition to fourth, fifth, of never.
    Instr,
}

impl FromStr for UsbAddress {
    type Err = UsbParseError;

    fn from_str(address: &str) -> Result<Self, Self::Err> {
        use UsbParseError::*;
        use UsbParserState::*;

        let mut addr_iter = address.char_indices().peekable();
        let mut buffer = String::with_capacity(10);
        let mut span = 0..0;

        let mut ret = Ok(UsbAddress {
            board: None,
            manufactuer_id: 0,
            model_code: 0,
            serial_number: String::new(),
            interface_number: None,
            instr: false,
        });
        let mut parser_state = Usb;

        while let Ok(resource) = &mut ret {
            if let Some((addr_index, addr_char)) = addr_iter.next() {
                span.end = addr_index;

                match (&parser_state, addr_char) {
                    (Usb, 'U') if addr_index == 0 => {
                        // USB[board]::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]
                        // ↑
                        // You are here
                        continue;
                    }
                    (Usb, 'S') if addr_index == 1 => {
                        // USB[board]::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]
                        //  ↑
                        // You are here
                        continue;
                    }
                    (Usb, 'B') if addr_index == 2 => {
                        // USB[board]::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]
                        //   ↑
                        // You are here
                        span.start = addr_index + 1;
                        buffer.clear();

                        parser_state = Board;
                        continue;
                    }
                    (Usb, _) => {
                        // USB[board]::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]
                        // ???
                        // You are here (Error)

                        ret = Err(NotUSB(address[0..3].to_string()));
                        break;
                    }
                    (Board, ':') if span.is_empty() => {
                        // USB::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]
                        //    ↑
                        // You are here (no board)
                        resource.board = None;

                        addr_iter.next();
                        span.start = addr_index + 2;
                        buffer.clear();

                        parser_state = ManufactuerId;
                        continue;
                    }
                    (Board, ':') => {
                        // USB[board]::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]
                        //           ↑
                        // You are here

                        match buffer.parse::<u32>() {
                            Ok(board_num) => {
                                resource.board = Some(board_num);

                                addr_iter.next(); // will be two colons I think.
                                span.start = addr_index + 2;
                                buffer.clear();

                                parser_state = ManufactuerId;
                                continue;
                            }
                            Err(err) => {
                                ret = Err(NumParseError {
                                    found: buffer,
                                    addr: address.to_string(),
                                    start: span.start,
                                    end: span.end,
                                    source: err,
                                });
                                break;
                            }
                        }
                    }
                    (ManufactuerId, ':') | (ModelCode, ':') => {
                        // USB[board]::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]
                        //                            ↑     OR    ↑
                        // You are here

                        // Parses hex number
                        match u16::from_str_radix(buffer.as_str(), 16) {
                            Ok(code) => {
                                // TODO: Once debug_assert_matches!() stabilizes use it here.
                                addr_iter.next(); // will be two colons.

                                // Advanced to where the start of the modelcode or serialnumber will be.
                                span.start = addr_index + 2;
                                buffer.clear();

                                parser_state = match parser_state {
                                    ManufactuerId => {
                                        resource.manufactuer_id = code;
                                        ModelCode
                                    }
                                    ModelCode => {
                                        resource.model_code = code;
                                        SerialNumber
                                    }
                                    _ => unreachable!(),
                                };

                                continue;
                            }
                            Err(err) => {
                                ret = Err(NumParseError {
                                    found: buffer,
                                    addr: address.to_string(),
                                    start: span.start,
                                    end: span.end,
                                    source: err,
                                });
                                break;
                            }
                        }
                    }
                    (ManufactuerId, char) | (ModelCode, char) if span.is_empty() => {
                        if char == '0' {
                            // USB[board]::0x<CODE>::0x<CODE>::serial number[::USB interfacenumber][::INSTR]
                            //             ↑    OR   ↑
                            // You are here

                            // Validates that this is a hex format
                            continue;
                        } else {
                            buffer.push(char);

                            ret = Err(NotHex {
                                found: 'scanning0: loop {
                                    if let Some((index, char)) = addr_iter.next() {
                                        span.end = index;
                                        if char == ':' {
                                            break buffer;
                                        } else {
                                            buffer.push(char);
                                        }
                                    } else {
                                        break 'scanning0 buffer;
                                    }
                                },
                                addr: address.to_string(),
                                start: span.start,
                                end: span.end,
                            });
                            break;
                        }
                    }
                    (ManufactuerId, char) | (ModelCode, char) if span.len() == 1 => {
                        // USB[board]::0x<CODE>::0x<CODE>::serial number[::USB interfacenumber][::INSTR]
                        //              ↑    OR   ↑
                        // You are here

                        if char == 'x' || char == 'X' {
                            continue;
                        } else {
                            buffer.push(char);

                            ret = Err(NotHex {
                                found: 'scanningX: loop {
                                    if let Some((index, char)) = addr_iter.next() {
                                        span.end = index;
                                        if char == ':' {
                                            break buffer;
                                        } else {
                                            buffer.push(char);
                                        }
                                    } else {
                                        break 'scanningX buffer;
                                    }
                                },
                                addr: address.to_string(),
                                start: span.start,
                                end: span.end,
                            });
                            break;
                        }
                    }
                    (SerialNumber, ':') => {
                        // USB[board]::0x<CODE>::0x<CODE>::serial number[::USB interfacenumber][::INSTR]
                        //                                               ↑          OR          ↑
                        // You are here

                        resource.serial_number.clone_from(&buffer);
                        buffer.clear();

                        addr_iter.next();
                        span.start = addr_index + 2;

                        // USB interface is optional so peek to see what's next.
                        parser_state = match addr_iter.peek() {
                            Some((_, 'I')) | Some((_, 'i')) => Instr,
                            _ => USBInterface,
                        };
                        continue;
                    }
                    (USBInterface, ':') => {
                        // USB[board]::0x<CODE>::0x<CODE>::serial number[::USB interfacenumber][::INSTR]
                        //                                                                      ↑
                        // You are here

                        match buffer.parse() {
                            Ok(num) => {
                                resource.interface_number = Some(num);
                                buffer.clear();

                                addr_iter.next();
                                span.start = addr_index + 2;
                                parser_state = Instr;
                                continue;
                            }
                            Err(err) => {
                                ret = Err(NumParseError {
                                    found: buffer,
                                    addr: address.to_string(),
                                    start: span.start,
                                    end: span.end,
                                    source: err,
                                });
                                break;
                            }
                        }
                    }
                    (Board, char)
                    | (ManufactuerId, char)
                    | (ModelCode, char)
                    | (SerialNumber, char)
                    | (USBInterface, char)
                    | (Instr, char) => {
                        // USB[board]::0x<CODE>::0x<CODE>::serial number[::USB interfacenumber][::INSTR]
                        //    ↑-----↑ OR  ↑---↑ OR ↑----↑OR↑-----------↑ OR↑------------------↑
                        // You are here

                        buffer.push(char);
                        continue;
                    }
                }
            } else {
                // What happens when the address ends?
                match parser_state {
                    Usb => {
                        ret = Err(IncompleteAddress(
                            address.to_string(),
                            "USB flag, Manufacture Code, Model Number, Serial number".to_string(),
                        ))
                    }
                    Board | ManufactuerId => {
                        ret = Err(IncompleteAddress(
                            address.to_string(),
                            "Manufacture Code, Model Number, Serial number".to_string(),
                        ))
                    }
                    ModelCode => {
                        ret = Err(IncompleteAddress(
                            address.to_string(),
                            "Model Number, Serial number".to_string(),
                        ))
                    }
                    SerialNumber => {
                        // USB[board]::manufacturer ID::model code::serial number
                        //                                                       ↑
                        // You are here

                        // I do not know what the proper format of a serial number is.
                        // So I'll just accept anything that is not an empty string.
                        match buffer.as_str() {
                            "" => {
                                ret = Err(IncompleteAddress(address.into(), "Serial Number".into()))
                            }
                            _ => resource.serial_number = buffer,
                        }
                    }
                    USBInterface => {
                        // USB[board]::manufacturer ID::model code::serial number::USB interfacenumber
                        //                                                                            ↑
                        // You are here

                        match buffer.parse() {
                            Ok(num) => resource.interface_number = Some(num),
                            Err(err) => {
                                ret = Err(NumParseError {
                                    found: buffer,
                                    addr: address.to_string(),
                                    start: span.start,
                                    end: span.end,
                                    source: err,
                                });
                                break;
                            }
                        }
                    }
                    Instr => {
                        // USB[board]::manufacturer ID::model code::serial number::USB interfacenumber::INSTR
                        //                                                                                   ↑
                        // You are here

                        let buff_upper = buffer.to_uppercase();

                        if buff_upper == "INSTR" {
                            resource.instr = true;
                        } else {
                            ret = Err(NotInstr {
                                found: buffer,
                                addr: address.to_string(),
                                start: span.start,
                                end: span.end,
                            })
                        }
                    }
                }
                break;
            }
        }
        ret
    }
}

impl Display for UsbAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Reference:
        // USB[board]::manufacturer ID::model code::serial number[::USB interfacenumber][::INSTR]

        let mut board_str = String::with_capacity(2);
        let mut interface_str = String::with_capacity(5);
        let mut instr_str = String::with_capacity(5);

        if let Some(num) = self.board {
            write!(board_str, "{}", num)?
        }
        if let Some(num) = self.interface_number {
            write!(interface_str, "::{}", num)?
        }
        if self.instr {
            instr_str.write_str("::INSTR")?
        }

        write!(
            f,
            "USB{}::{:#X}::{:#X}::{}{}{}",
            board_str,
            self.manufactuer_id,
            self.model_code,
            self.serial_number,
            interface_str,
            instr_str
        )
    }
}

#[cfg(test)]
mod test {
    //! Different permutations of USB addresses to parse.
    use super::*;

    /// Helper macro
    /// test_parse!(function_identifier, address_to_parse);
    macro_rules! test_parse {
        ($name:ident, $addr:literal) => {
            #[test]
            fn $name() -> Result<(), UsbParseError> {
                const ADDR: &str = $addr;
                match UsbAddress::from_str(ADDR) {
                    Ok(address) => {
                        assert_eq!(address.to_string(), ADDR);
                        Ok(())
                    }
                    Err(err) => Err(err),
                }
            }
        };
    }

    test_parse!(usb_parse_address, "USB::0x1A34::0x5678::A22-5");
    test_parse!(usb_parse_board, "USB1::0x12B4::0x56F8::A22-5::INSTR");
    test_parse!(usb_parse_instr, "USB::0xFFA1::0x56C8::A22-5::INSTR");
    test_parse!(usb_parse_interface, "USB::0x1234::0x5D78::A22-5::123");
    test_parse!(usb_parse_all, "USB34::0x12A4::0xFF1A::A22-5::12314::INSTR");

    mod ui {
        //! USB Address UI tests.
        use super::*;

        /// Helper macro
        /// test_ui!(function_identifier, address_to_parse, expected_error);
        macro_rules! test_ui {
            ($name:ident, $addr:literal, $expected:literal) => {
                #[test]
                fn $name() -> Result<(), String> {
                    const ADDR: &str = $addr;
                    const EXPECT: &str = $expected;
                    if let Err(err) = UsbAddress::from_str(ADDR) {
                        if err.to_string() == EXPECT {
                            Ok(())
                        } else {
                            Err(format!("Incorrect error returned:\n {err}"))
                        }
                    } else {
                        Err(format!("Accepted invalid USB address: {ADDR}").into())
                    }
                }
            };
        }

        test_ui!(
            usb_ui_not_usb,
            "TCPIP::1.2.3.4::inst0::INSTR",
            "Expected \"USB\" at address start, found \"TCP\""
        );
        test_ui!(usb_ui_cut_usb, "US", "\"US\" is an incomplete address missing: USB flag, Manufacture Code, Model Number, Serial number");
        test_ui!(usb_ui_cut_manu, "USB::0x", "\"USB::0x\" is an incomplete address missing: Manufacture Code, Model Number, Serial number");
        test_ui!(
            usb_ui_cut_model,
            "USB::0x321::0x1",
            "\"USB::0x321::0x1\" is an incomplete address missing: Model Number, Serial number"
        );
        test_ui!(
            usb_ui_cut_serial,
            "USB::0x321::0x132::",
            "\"USB::0x321::0x132::\" is an incomplete address missing: Serial Number"
        );
        test_ui!(usb_ui_manu_hex, "USB34::x1H34::0x5678::A22-5::12314::INSTR", "Invalid hexidecimal number: \"x1H34\" at position 7 to 12 in\n \"USB34::x1H34::0x5678::A22-5::12314::INSTR\"\nNumber must start with '0x'");
        test_ui!(usb_ui_model_hex, "USB34::0x1B34::x56A8::A22-5::12314::INSTR", "Invalid hexidecimal number: \"x56A8\" at position 15 to 20 in\n \"USB34::0x1B34::x56A8::A22-5::12314::INSTR\"\nNumber must start with '0x'");
        test_ui!(usb_ui_wrong_inst_long, "USB34::0x12C4::0x5678::A22-5::12314::INSTRfdss", "In address \"INSTR\" was indicated but instead \"INSTRfdss\" was found at 37 to 45 of\n \"USB34::0x12C4::0x5678::A22-5::12314::INSTRfdss\"");
        test_ui!(usb_ui_wrong_inst_short, "USB34::0x1234::0x5D78::A22-5::INST", "In address \"INSTR\" was indicated but instead \"INST\" was found at 30 to 33 of\n \"USB34::0x1234::0x5D78::A22-5::INST\"");
        test_ui!(usb_ui_num_err_model, "USB34::0x1234::0x56Z8::A22-5::12314::INSTR", "Found \"56Z8\" instead of a number at position 15 to 21 of \n\"USB34::0x1234::0x56Z8::A22-5::12314::INSTR\"");
        test_ui!(usb_ui_num_err_manu, "USB34::0xTEST::0x568::A22-5::12314::INSTR", "Found \"TEST\" instead of a number at position 7 to 13 of \n\"USB34::0xTEST::0x568::A22-5::12314::INSTR\"");
    }
}
