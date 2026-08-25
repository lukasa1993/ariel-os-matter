//! Matter 1.6 device-type catalog.
//!
//! The only excluded standard device types are Camera, Video Doorbell, and
//! Robotic Vacuum Cleaner. Device-family Cargo features control the runtime
//! support table. Marker types are zero-sized and add no runtime storage.

use heapless::Vec;

use crate::{Error, Result};

/// Maximum number of enabled Matter device types.
pub const MAX_DEVICE_TYPES: usize = 128;

/// Broad implementation family used for Cargo feature selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceFamily {
    /// Root, labels, modes, power sources, and other foundation types.
    Foundation,
    /// Lights and plug-in loads.
    Lighting,
    /// Switches and mounted controls.
    Switches,
    /// Environmental, safety, and contact sensors.
    Sensors,
    /// Locks, windows, and closures.
    LocksClosures,
    /// Thermostats, fans, air treatment, and pumps.
    Hvac,
    /// Valves, irrigation, leak, freeze, rain, and soil devices.
    Water,
    /// Energy management, metering, storage, and EVSE.
    Energy,
    /// Major appliances.
    Appliances,
    /// Audio and media devices.
    Media,
    /// Bridges and aggregators.
    Bridges,
    /// Thread infrastructure.
    ThreadInfrastructure,
    /// OTA roles.
    Ota,
}

/// Matter device-type metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceTypeDescriptor {
    /// Matter device-type identifier.
    pub id: u32,
    /// Device Library revision used by the adapter.
    pub revision: u16,
    /// Standard name.
    pub name: &'static str,
    /// Feature family.
    pub family: DeviceFamily,
}

/// Marker implemented by every typed device API.
pub trait DeviceTypeMarker {
    /// Standard descriptor.
    const DESCRIPTOR: DeviceTypeDescriptor;
}

macro_rules! device_type {
    ($marker:ident, $id:expr, $revision:expr, $name:literal, $family:ident) => {
        #[doc = concat!("Typed marker for the Matter `", $name, "` device type.")]
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
        pub struct $marker;

        impl DeviceTypeMarker for $marker {
            const DESCRIPTOR: DeviceTypeDescriptor = DeviceTypeDescriptor {
                id: $id,
                revision: $revision,
                name: $name,
                family: DeviceFamily::$family,
            };
        }
    };
}

device_type!(DoorLock, 0x000A, 1, "Door Lock", LocksClosures);
device_type!(
    DoorLockController,
    0x000B,
    1,
    "Door Lock Controller",
    LocksClosures
);
device_type!(Aggregator, 0x000E, 2, "Aggregator", Bridges);
device_type!(GenericSwitch, 0x000F, 2, "Generic Switch", Switches);
device_type!(PowerSource, 0x0011, 1, "Power Source", Foundation);
device_type!(OtaRequestor, 0x0012, 1, "OTA Requestor", Ota);
device_type!(BridgedNode, 0x0013, 1, "Bridged Node", Bridges);
device_type!(OtaProvider, 0x0014, 1, "OTA Provider", Ota);
device_type!(ContactSensor, 0x0015, 1, "Contact Sensor", Sensors);
device_type!(RootNode, 0x0016, 5, "Root Node", Foundation);
device_type!(SolarPower, 0x0017, 1, "Solar Power", Energy);
device_type!(BatteryStorage, 0x0018, 1, "Battery Storage", Energy);
device_type!(
    SecondaryNetworkInterface,
    0x0019,
    1,
    "Secondary Network Interface",
    Foundation
);
device_type!(Speaker, 0x0022, 2, "Speaker", Media);
device_type!(CastingVideoPlayer, 0x0023, 1, "Casting Video Player", Media);
device_type!(ContentApp, 0x0024, 1, "Content App", Media);
device_type!(ModeSelect, 0x0027, 1, "Mode Select", Foundation);
device_type!(BasicVideoPlayer, 0x0028, 1, "Basic Video Player", Media);
device_type!(CastingVideoClient, 0x0029, 1, "Casting Video Client", Media);
device_type!(VideoRemoteControl, 0x002A, 1, "Video Remote Control", Media);
device_type!(Fan, 0x002B, 1, "Fan", Hvac);
device_type!(AirQualitySensor, 0x002C, 1, "Air Quality Sensor", Sensors);
device_type!(AirPurifier, 0x002D, 1, "Air Purifier", Hvac);
device_type!(IrrigationSystem, 0x0040, 1, "Irrigation System", Water);
device_type!(
    WaterFreezeDetector,
    0x0041,
    1,
    "Water Freeze Detector",
    Water
);
device_type!(WaterValve, 0x0042, 1, "Water Valve", Water);
device_type!(WaterLeakDetector, 0x0043, 1, "Water Leak Detector", Water);
device_type!(RainSensor, 0x0044, 1, "Rain Sensor", Water);
device_type!(SoilSensor, 0x0045, 1, "Soil Sensor", Water);
device_type!(Refrigerator, 0x0070, 1, "Refrigerator", Appliances);
device_type!(
    TemperatureControlledCabinet,
    0x0071,
    1,
    "Temperature Controlled Cabinet",
    Appliances
);
device_type!(
    RoomAirConditioner,
    0x0072,
    1,
    "Room Air Conditioner",
    Appliances
);
device_type!(LaundryWasher, 0x0073, 1, "Laundry Washer", Appliances);
device_type!(Dishwasher, 0x0075, 1, "Dishwasher", Appliances);
device_type!(SmokeCoAlarm, 0x0076, 1, "Smoke CO Alarm", Sensors);
device_type!(CookSurface, 0x0077, 1, "Cook Surface", Appliances);
device_type!(Cooktop, 0x0078, 1, "Cooktop", Appliances);
device_type!(MicrowaveOven, 0x0079, 1, "Microwave Oven", Appliances);
device_type!(ExtractorHood, 0x007A, 1, "Extractor Hood", Appliances);
device_type!(Oven, 0x007B, 1, "Oven", Appliances);
device_type!(LaundryDryer, 0x007C, 1, "Laundry Dryer", Appliances);
device_type!(
    NetworkInfrastructureManager,
    0x0090,
    1,
    "Network Infrastructure Manager",
    ThreadInfrastructure
);
device_type!(
    ThreadBorderRouter,
    0x0091,
    1,
    "Thread Border Router",
    ThreadInfrastructure
);
device_type!(OnOffLight, 0x0100, 3, "On/Off Light", Lighting);
device_type!(DimmableLight, 0x0101, 3, "Dimmable Light", Lighting);
device_type!(OnOffLightSwitch, 0x0103, 3, "On/Off Light Switch", Switches);
device_type!(DimmerSwitch, 0x0104, 3, "Dimmer Switch", Switches);
device_type!(
    ColorDimmerSwitch,
    0x0105,
    3,
    "Color Dimmer Switch",
    Switches
);
device_type!(LightSensor, 0x0106, 2, "Light Sensor", Sensors);
device_type!(OccupancySensor, 0x0107, 2, "Occupancy Sensor", Sensors);
device_type!(OnOffPluginUnit, 0x010A, 3, "On/Off Plug-in Unit", Lighting);
device_type!(
    DimmablePluginUnit,
    0x010B,
    3,
    "Dimmable Plug-in Unit",
    Lighting
);
device_type!(
    ColorTemperatureLight,
    0x010C,
    3,
    "Color Temperature Light",
    Lighting
);
device_type!(
    ExtendedColorLight,
    0x010D,
    4,
    "Extended Color Light",
    Lighting
);
device_type!(
    MountedOnOffControl,
    0x010F,
    1,
    "Mounted On/Off Control",
    Switches
);
device_type!(
    MountedDimmableLoadControl,
    0x0110,
    1,
    "Mounted Dimmable Load Control",
    Switches
);
device_type!(
    JointFabricAdministrator,
    0x0130,
    1,
    "Joint Fabric Administrator",
    Foundation
);
device_type!(Intercom, 0x0140, 1, "Intercom", Media);
device_type!(AudioDoorbell, 0x0141, 1, "Audio Doorbell", Media);
device_type!(FloodlightCamera, 0x0144, 1, "Floodlight Camera", Lighting);
device_type!(SnapshotCamera, 0x0145, 1, "Snapshot Camera", Sensors);
device_type!(Chime, 0x0146, 1, "Chime", Media);
device_type!(CameraController, 0x0147, 1, "Camera Controller", Media);
device_type!(Doorbell, 0x0148, 1, "Doorbell", Sensors);
device_type!(WindowCovering, 0x0202, 2, "Window Covering", LocksClosures);
device_type!(
    WindowCoveringController,
    0x0203,
    2,
    "Window Covering Controller",
    LocksClosures
);
device_type!(Closure, 0x0230, 1, "Closure", LocksClosures);
device_type!(ClosurePanel, 0x0231, 1, "Closure Panel", LocksClosures);
device_type!(
    ClosureController,
    0x023E,
    1,
    "Closure Controller",
    LocksClosures
);
device_type!(Thermostat, 0x0301, 3, "Thermostat", Hvac);
device_type!(TemperatureSensor, 0x0302, 2, "Temperature Sensor", Sensors);
device_type!(Pump, 0x0303, 2, "Pump", Hvac);
device_type!(PumpController, 0x0304, 1, "Pump Controller", Hvac);
device_type!(PressureSensor, 0x0305, 1, "Pressure Sensor", Sensors);
device_type!(FlowSensor, 0x0306, 1, "Flow Sensor", Sensors);
device_type!(HumiditySensor, 0x0307, 2, "Humidity Sensor", Sensors);
device_type!(HeatPump, 0x0309, 1, "Heat Pump", Hvac);
device_type!(
    ThermostatController,
    0x030A,
    1,
    "Thermostat Controller",
    Hvac
);
device_type!(EnergyEvse, 0x050C, 2, "Energy EVSE", Energy);
device_type!(
    DeviceEnergyManagement,
    0x050D,
    2,
    "Device Energy Management",
    Energy
);
device_type!(WaterHeater, 0x050F, 1, "Water Heater", Energy);
device_type!(ElectricalSensor, 0x0510, 2, "Electrical Sensor", Energy);
device_type!(
    ElectricalUtilityMeter,
    0x0511,
    1,
    "Electrical Utility Meter",
    Energy
);
device_type!(
    MeterReferencePoint,
    0x0512,
    1,
    "Meter Reference Point",
    Energy
);
device_type!(
    ElectricalEnergyTariff,
    0x0513,
    1,
    "Electrical Energy Tariff",
    Energy
);
device_type!(ElectricalMeter, 0x0514, 1, "Electrical Meter", Energy);
device_type!(ControlBridge, 0x0840, 1, "Control Bridge", Bridges);
device_type!(OnOffSensor, 0x0850, 1, "On/Off Sensor", Sensors);

/// The exact excluded set required by this project.
pub const EXCLUDED_DEVICE_TYPES: [DeviceTypeDescriptor; 3] = [
    DeviceTypeDescriptor {
        id: 0x0074,
        revision: 1,
        name: "Robotic Vacuum Cleaner",
        family: DeviceFamily::Appliances,
    },
    DeviceTypeDescriptor {
        id: 0x0142,
        revision: 1,
        name: "Camera",
        family: DeviceFamily::Media,
    },
    DeviceTypeDescriptor {
        id: 0x0143,
        revision: 1,
        name: "Video Doorbell",
        family: DeviceFamily::Media,
    },
];

macro_rules! push_type {
    ($output:ident, $marker:ty) => {
        $output
            .push(<$marker as DeviceTypeMarker>::DESCRIPTOR)
            .map_err(|_| Error::Capacity)?;
    };
}

/// Build the device table selected by Cargo features.
pub fn enabled_device_types() -> Result<Vec<DeviceTypeDescriptor, MAX_DEVICE_TYPES>> {
    let mut output = Vec::new();

    #[cfg(feature = "device-foundation")]
    {
        push_type!(output, PowerSource);
        push_type!(output, RootNode);
        push_type!(output, SecondaryNetworkInterface);
        push_type!(output, ModeSelect);
        push_type!(output, JointFabricAdministrator);
    }
    #[cfg(feature = "device-lighting")]
    {
        push_type!(output, OnOffLight);
        push_type!(output, DimmableLight);
        push_type!(output, OnOffPluginUnit);
        push_type!(output, DimmablePluginUnit);
        push_type!(output, ColorTemperatureLight);
        push_type!(output, ExtendedColorLight);
        push_type!(output, FloodlightCamera);
    }
    #[cfg(feature = "device-switches")]
    {
        push_type!(output, GenericSwitch);
        push_type!(output, OnOffLightSwitch);
        push_type!(output, DimmerSwitch);
        push_type!(output, ColorDimmerSwitch);
        push_type!(output, MountedOnOffControl);
        push_type!(output, MountedDimmableLoadControl);
    }
    #[cfg(feature = "device-sensors")]
    {
        push_type!(output, ContactSensor);
        push_type!(output, AirQualitySensor);
        push_type!(output, SmokeCoAlarm);
        push_type!(output, LightSensor);
        push_type!(output, OccupancySensor);
        push_type!(output, SnapshotCamera);
        push_type!(output, Doorbell);
        push_type!(output, TemperatureSensor);
        push_type!(output, PressureSensor);
        push_type!(output, FlowSensor);
        push_type!(output, HumiditySensor);
        push_type!(output, OnOffSensor);
    }
    #[cfg(feature = "device-locks-closures")]
    {
        push_type!(output, DoorLock);
        push_type!(output, DoorLockController);
        push_type!(output, WindowCovering);
        push_type!(output, WindowCoveringController);
        push_type!(output, Closure);
        push_type!(output, ClosurePanel);
        push_type!(output, ClosureController);
    }
    #[cfg(feature = "device-hvac")]
    {
        push_type!(output, Fan);
        push_type!(output, AirPurifier);
        push_type!(output, Thermostat);
        push_type!(output, Pump);
        push_type!(output, PumpController);
        push_type!(output, HeatPump);
        push_type!(output, ThermostatController);
        push_type!(output, RoomAirConditioner);
    }
    #[cfg(feature = "device-water")]
    {
        push_type!(output, IrrigationSystem);
        push_type!(output, WaterFreezeDetector);
        push_type!(output, WaterValve);
        push_type!(output, WaterLeakDetector);
        push_type!(output, RainSensor);
        push_type!(output, SoilSensor);
    }
    #[cfg(feature = "device-energy")]
    {
        push_type!(output, SolarPower);
        push_type!(output, BatteryStorage);
        push_type!(output, EnergyEvse);
        push_type!(output, DeviceEnergyManagement);
        push_type!(output, WaterHeater);
        push_type!(output, ElectricalSensor);
        push_type!(output, ElectricalUtilityMeter);
        push_type!(output, MeterReferencePoint);
        push_type!(output, ElectricalEnergyTariff);
        push_type!(output, ElectricalMeter);
    }
    #[cfg(feature = "device-appliances")]
    {
        push_type!(output, Refrigerator);
        push_type!(output, TemperatureControlledCabinet);
        push_type!(output, LaundryWasher);
        push_type!(output, Dishwasher);
        push_type!(output, CookSurface);
        push_type!(output, Cooktop);
        push_type!(output, MicrowaveOven);
        push_type!(output, ExtractorHood);
        push_type!(output, Oven);
        push_type!(output, LaundryDryer);
    }
    #[cfg(feature = "device-media")]
    {
        push_type!(output, Speaker);
        push_type!(output, CastingVideoPlayer);
        push_type!(output, ContentApp);
        push_type!(output, BasicVideoPlayer);
        push_type!(output, CastingVideoClient);
        push_type!(output, VideoRemoteControl);
        push_type!(output, Intercom);
        push_type!(output, AudioDoorbell);
        push_type!(output, Chime);
        push_type!(output, CameraController);
    }
    #[cfg(feature = "device-bridges")]
    {
        push_type!(output, Aggregator);
        push_type!(output, BridgedNode);
        push_type!(output, ControlBridge);
    }
    #[cfg(feature = "device-thread-infrastructure")]
    {
        push_type!(output, NetworkInfrastructureManager);
        push_type!(output, ThreadBorderRouter);
    }
    #[cfg(feature = "device-ota")]
    {
        push_type!(output, OtaRequestor);
        push_type!(output, OtaProvider);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_exclusion_set_is_stable() {
        assert_eq!(EXCLUDED_DEVICE_TYPES.len(), 3);
        assert!(EXCLUDED_DEVICE_TYPES.iter().any(|value| value.id == 0x0074));
        assert!(EXCLUDED_DEVICE_TYPES.iter().any(|value| value.id == 0x0142));
        assert!(EXCLUDED_DEVICE_TYPES.iter().any(|value| value.id == 0x0143));
    }

    #[cfg(feature = "all-device-types")]
    #[test]
    fn enabled_catalog_has_no_duplicate_or_excluded_id() {
        let catalog = enabled_device_types();
        assert!(catalog.is_ok());
        if let Ok(catalog) = catalog {
            for (index, device) in catalog.iter().enumerate() {
                assert!(
                    !EXCLUDED_DEVICE_TYPES
                        .iter()
                        .any(|excluded| excluded.id == device.id)
                );
                assert!(
                    !catalog
                        .iter()
                        .skip(index + 1)
                        .any(|other| other.id == device.id)
                );
            }
            assert_eq!(catalog.len(), 86);
        }
    }
}
