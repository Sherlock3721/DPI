use std::f64::consts::PI;

/// Výpočet extruze pro laboratorní tisk. Převádí objem v µl (což odpovídá mm³)
/// nebo kroky na délku vytlačovaného filamentu v mm.
pub struct ExtrusionCalculator {
    pub filament_diameter: f64,
    pub flow_multiplier: f64,
    pub calibration_factor: f64,
}

impl ExtrusionCalculator {
    /// Vytvoří novou instanci. Pokud není zadán kalibrační faktor (calibration_factor),
    /// automaticky se spočítá z průměru filamentu (1 µl = 1 mm³ objemu).
    pub fn new(
        filament_diameter: f64,
        flow_multiplier: f64,
        calibration_factor: Option<f64>,
    ) -> Self {
        let radius = filament_diameter / 2.0;
        let area = PI * radius.powi(2);

        let cal_factor = match calibration_factor {
            Some(cal) => cal,
            None => 1.0 / area,
        };

        Self {
            filament_diameter,
            flow_multiplier,
            calibration_factor: cal_factor,
        }
    }

    /// Spočítá délku vytlačeného filamentu (E v mm) na 1 mm tiskové dráhy.
    pub fn calculate_e_per_mm(&self, extrusion_rate: f64, unit: &str) -> f64 {
        match unit {
            "kroky/mm" => {
                // Přímé kroky na mm; multiplikátor průtoku platí stejně jako
                // u objemových jednotek (frontend převádí nl ↔ kroky bez něj)
                extrusion_rate * self.flow_multiplier
            }
            "nl/mm" => {
                // 1 nl = 0.001 µl (mm³) objemu kapaliny
                let volume_per_mm = (extrusion_rate / 1000.0) * self.flow_multiplier;
                volume_per_mm * self.calibration_factor
            }
            "µl/mm" => {
                // 1 µl = 1 mm³ objemu kapaliny
                let volume_per_mm = extrusion_rate * self.flow_multiplier;
                volume_per_mm * self.calibration_factor
            }
            _ => {
                // Výchozí je µl/mm
                let volume_per_mm = extrusion_rate * self.flow_multiplier;
                volume_per_mm * self.calibration_factor
            }
        }
    }

    /// Spočítá délku vytlačeného filamentu (E v mm) pro jednu kapku (tečku).
    pub fn calculate_dot_extrusion(&self, rate: f64, unit: &str) -> f64 {
        match unit {
            "kroky" | "kroky/mm" => rate * self.flow_multiplier,
            "nl" | "nl/mm" => {
                let volume = (rate / 1000.0) * self.flow_multiplier;
                volume * self.calibration_factor
            }
            "µl" | "µl/mm" => {
                let volume = rate * self.flow_multiplier;
                volume * self.calibration_factor
            }
            _ => {
                // Výchozí je µl
                let volume = rate * self.flow_multiplier;
                volume * self.calibration_factor
            }
        }
    }
}
