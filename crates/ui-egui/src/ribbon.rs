//! The Ribbon (cinta de opciones superior horizontal): tabbed command bar organizing CADCraft's
//! drafting, modeling, annotation, parametric, view, management, output, and utility tools.

use cadcraft_color::Color;
use cadcraft_engine::Session;
use egui::{Rect, Sense, Stroke, pos2, vec2};

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

#[derive(Clone, Copy, Debug)]
pub enum RibbonItem {
    /// Large button with icon on top and label below (52x64).
    Large { icon: Icon, id: &'static str, label: &'static str, tip: &'static str },
    /// Split button: main button executes default command on click; dropdown arrow opens popup with variants.
    SplitButton { icon: Icon, id: &'static str, label: &'static str, tip: &'static str, menu: &'static [(&'static str, &'static str, Icon)] },
    /// Large button with a dropdown of alternative/related commands.
    LargeDropdown { icon: Icon, id: &'static str, label: &'static str, tip: &'static str, menu: &'static [(&'static str, &'static str, Icon)] },
    /// Medium button grid (e.g. 3 rows x N columns): icon left, label right.
    MediumGrid { cols: &'static [&'static [MediumItem]] },
    /// Column of up to 3 compact items stacked vertically.
    CompactColumn(&'static [CompactItem]),
    /// Column of up to 3 icon-only compact items stacked vertically.
    IconColumn(&'static [IconItem]),
    /// Real dynamic Layer control panel item (Current layer combobox + quick controls).
    LayersControl,
    /// Real dynamic Properties control panel item (Color, Lineweight, Linetype).
    PropertiesControl,
}

#[derive(Clone, Copy, Debug)]
pub enum MediumItem {
    Button { icon: Icon, id: &'static str, label: &'static str, tip: &'static str },
    Dropdown { icon: Icon, id: &'static str, label: &'static str, tip: &'static str, menu: &'static [(&'static str, &'static str, Icon)] },
}

#[derive(Clone, Copy, Debug)]
pub struct IconItem {
    pub icon: Icon,
    pub id: &'static str,
    pub tip: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub enum CompactItem {
    Button { icon: Icon, id: &'static str, label: &'static str, tip: &'static str },
    Dropdown { icon: Icon, id: &'static str, label: &'static str, tip: &'static str, menu: &'static [(&'static str, &'static str, Icon)] },
    IconOnly { icon: Icon, id: &'static str, tip: &'static str },
}

#[derive(Clone, Copy, Debug)]
pub struct RibbonPanel {
    pub title: &'static str,
    pub items: &'static [RibbonItem],
    pub more: &'static [(&'static str, &'static str, Icon)],
}

#[derive(Clone, Copy, Debug)]
pub struct RibbonTab {
    pub id: &'static str,
    pub label: &'static str,
    pub panels: &'static [RibbonPanel],
}

pub const TABS: &[RibbonTab] = &[
    RibbonTab {
        id: "Home",
        label: "Home",
        panels: &[
            RibbonPanel {
                title: "Draw",
                items: &[
                    RibbonItem::Large { icon: Icon::Line, id: "line", label: "Line", tip: "Line (L)" },
                    RibbonItem::Large { icon: Icon::Polyline, id: "pline", label: "Polyline", tip: "Polyline (PL)" },
                    RibbonItem::SplitButton {
                        icon: Icon::Circle,
                        id: "circle",
                        label: "Circle",
                        tip: "Circle (C)",
                        menu: &[
                            ("circle", "Center, Radius", Icon::Circle),
                            ("circle.cd", "Center, Diameter", Icon::Circle),
                            ("circle.2p", "2 Points", Icon::Circle),
                            ("circle.3p", "3 Points", Icon::Circle),
                            ("circle.ttr", "Tan, Tan, Radius", Icon::Circle),
                            ("circle.ttt", "Tan, Tan, Tan", Icon::Circle),
                        ],
                    },
                    RibbonItem::SplitButton { icon: Icon::Arc, id: "arc", label: "Arc", tip: "Arc (A)", menu: &[("arc", "3-Point Arc", Icon::Arc)] },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Dropdown {
                            icon: Icon::Rectangle,
                            id: "rectang",
                            label: "",
                            tip: "Rectangle (REC)",
                            menu: &[("rectang", "Rectangle", Icon::Rectangle), ("polygon", "Polygon", Icon::Polygon)],
                        },
                        CompactItem::Button { icon: Icon::Ellipse, id: "ellipse", label: "", tip: "Center Ellipse (EL)" },
                        CompactItem::Dropdown {
                            icon: Icon::Hatch,
                            id: "hatch",
                            label: "",
                            tip: "Hatch (H)",
                            menu: &[
                                ("hatch", "Hatch", Icon::Hatch),
                                ("hatch.user", "User-Defined Hatch", Icon::Hatch),
                                ("hatch.diag45", "Diagonal 45° - separación 4", Icon::Hatch),
                                ("gradient", "Gradient", Icon::Gradient),
                                ("boundary", "Boundary", Icon::Boundary),
                            ],
                        },
                    ]),
                ],
                more: &[
                    ("spline", "Spline Fit", Icon::Spline),
                    ("spline.cv", "Spline CV", Icon::Spline),
                    ("xline", "Construction Line", Icon::XLine),
                    ("ray", "Ray", Icon::Ray),
                    ("revcloud", "Revision Cloud", Icon::RevCloud),
                    ("region", "Region", Icon::Region),
                    ("wipeout", "Wipeout", Icon::Wipeout),
                    ("point.multiple", "Multiple Point", Icon::Point),
                    ("donut", "Donut", Icon::Donut),
                ],
            },
            RibbonPanel {
                title: "Modify",
                items: &[
                    RibbonItem::MediumGrid {
                        cols: &[
                            &[
                                MediumItem::Button { icon: Icon::Move, id: "move", label: "Move", tip: "Move (M)" },
                                MediumItem::Button { icon: Icon::Copy, id: "copy", label: "Copy", tip: "Copy (CO)" },
                                MediumItem::Button { icon: Icon::Stretch, id: "stretch", label: "Stretch", tip: "Stretch (S)" },
                            ],
                            &[
                                MediumItem::Button { icon: Icon::Rotate, id: "rotate", label: "Rotate", tip: "Rotate (RO)" },
                                MediumItem::Button { icon: Icon::Mirror, id: "mirror", label: "Mirror", tip: "Mirror (MI)" },
                                MediumItem::Button { icon: Icon::Scale, id: "scale", label: "Scale", tip: "Scale (SC)" },
                            ],
                            &[
                                MediumItem::Dropdown {
                                    icon: Icon::Trim,
                                    id: "trim",
                                    label: "Trim",
                                    tip: "Trim (TR)",
                                    menu: &[("trim", "Trim", Icon::Trim), ("extend", "Extend", Icon::Extend)],
                                },
                                MediumItem::Dropdown {
                                    icon: Icon::Fillet,
                                    id: "fillet",
                                    label: "Fillet",
                                    tip: "Fillet (F)",
                                    menu: &[("fillet", "Fillet", Icon::Fillet), ("chamfer", "Chamfer", Icon::Chamfer)],
                                },
                                MediumItem::Dropdown {
                                    icon: Icon::ArrayRect,
                                    id: "arrayrect",
                                    label: "Array",
                                    tip: "Array",
                                    menu: &[("arrayrect", "Rectangular Array", Icon::ArrayRect), ("arraypolar", "Polar Array", Icon::ArrayPolar)],
                                },
                            ],
                        ],
                    },
                    RibbonItem::IconColumn(&[
                        IconItem { icon: Icon::Erase, id: "erase", tip: "Erase (E)" },
                        IconItem { icon: Icon::Explode, id: "explode", tip: "Explode (X)" },
                        IconItem { icon: Icon::Offset, id: "offset", tip: "Offset (O)" },
                    ]),
                ],
                more: &[
                    ("break", "Break", Icon::Break),
                    ("join", "Join", Icon::Join),
                    ("lengthen", "Lengthen", Icon::Lengthen),
                    ("matchprop", "Match Properties", Icon::MatchProp),
                    ("union", "Union", Icon::Join),
                    ("subtract", "Subtract", Icon::Trim),
                    ("intersect", "Intersect", Icon::Region),
                ],
            },
            RibbonPanel {
                title: "Annotation",
                items: &[
                    RibbonItem::SplitButton {
                        icon: Icon::MText,
                        id: "mtext",
                        label: "Text",
                        tip: "Multiline Text (MT)",
                        menu: &[("mtext", "Multiline Text", Icon::MText), ("text", "Single Line Text", Icon::Text)],
                    },
                    RibbonItem::Large { icon: Icon::DimLinear, id: "dimlinear", label: "Dimension", tip: "Linear Dimension (DLI)" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Dropdown {
                            icon: Icon::DimLinear,
                            id: "dimlinear",
                            label: "Linear",
                            tip: "Dimension Types",
                            menu: &[
                                ("dimlinear", "Linear", Icon::DimLinear),
                                ("dimaligned", "Aligned", Icon::DimAligned),
                                ("dimangular", "Angular", Icon::DimAngular),
                                ("dimradius", "Radius", Icon::DimRadius),
                                ("dimdiameter", "Diameter", Icon::DimDiameter),
                                ("dimarc", "Arc Length", Icon::DimArc),
                                ("dimcontinue", "Continue", Icon::DimContinue),
                                ("dimbaseline", "Baseline", Icon::DimBaseline),
                                ("dimordinate", "Ordinate", Icon::DimOrdinate),
                                ("qdim", "Quick Dimension", Icon::QDim),
                            ],
                        },
                        CompactItem::Dropdown {
                            icon: Icon::MLeader,
                            id: "mleader",
                            label: "Leader",
                            tip: "Multileader (MLD)",
                            menu: &[
                                ("mleader", "Multileader", Icon::MLeader),
                                ("mleaderedit.add", "Add Leader", Icon::AddLeader),
                                ("mleaderedit.remove", "Remove Leader", Icon::RemoveLeader),
                                ("mleaderalign", "Align Leaders", Icon::AlignLeaders),
                            ],
                        },
                        CompactItem::Button { icon: Icon::Table, id: "table", label: "Table", tip: "Table" },
                    ]),
                ],
                more: &[("style", "Text Style", Icon::Text), ("tablestyle", "Table Style", Icon::Table)],
            },
            RibbonPanel {
                title: "Layers",
                items: &[
                    RibbonItem::Large { icon: Icon::Layers, id: "ui.dialog.layers", label: "Layers", tip: "Layer Properties Manager" },
                    RibbonItem::LayersControl,
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Block",
                items: &[
                    RibbonItem::Large { icon: Icon::Insert, id: "insert", label: "Insert", tip: "Insert Block (I)" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::BlockCreate, id: "block", label: "Create", tip: "Create Block (B)" },
                        CompactItem::Button { icon: Icon::BlockEdit, id: "bedit", label: "Editor", tip: "Block Editor (BE)" },
                        CompactItem::Button { icon: Icon::AttDef, id: "attdef", label: "Attributes", tip: "Define Attributes" },
                    ]),
                ],
                more: &[("ui.dialog.blocks", "Blocks Palette", Icon::Blocks)],
            },
            RibbonPanel {
                title: "Properties",
                items: &[
                    RibbonItem::Large { icon: Icon::MatchProp, id: "matchprop", label: "Match Prop", tip: "Match Properties (MA)" },
                    RibbonItem::PropertiesControl,
                ],
                more: &[
                    ("ui.panel.properties", "Properties Inspector", Icon::Properties),
                    ("transparencydisplay", "Transparency Display", Icon::Transparency),
                ],
            },
            RibbonPanel {
                title: "Utilities",
                items: &[
                    RibbonItem::SplitButton {
                        icon: Icon::Measure,
                        id: "dist",
                        label: "Measure",
                        tip: "Measure Distance",
                        menu: &[("dist", "Measure Distance", Icon::Measure), ("area", "Measure Area", Icon::Area)],
                    },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Area, id: "area", label: "Area", tip: "Measure Area" },
                        CompactItem::Button {
                            icon: Icon::QuickSelect,
                            id: "ui.dialog.qselect",
                            label: "Quick Select",
                            tip: "Quick Select (QSELECT)",
                        },
                        CompactItem::Button { icon: Icon::List, id: "list", label: "List", tip: "List Database Information (LI)" },
                    ]),
                ],
                more: &[("id", "ID Point", Icon::Point), ("massprop", "Mass Properties", Icon::Properties)],
            },
        ],
    },
    RibbonTab {
        id: "Insert",
        label: "Insert",
        panels: &[
            RibbonPanel {
                title: "Block",
                items: &[
                    RibbonItem::Large { icon: Icon::Insert, id: "insert", label: "Insert", tip: "Insert Block" },
                    RibbonItem::Large { icon: Icon::BlockCreate, id: "block", label: "Create", tip: "Create Block" },
                    RibbonItem::Large { icon: Icon::BlockEdit, id: "bedit", label: "Edit", tip: "Block Editor" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::AttDef, id: "attdef", label: "Attributes", tip: "Define Attributes" },
                        CompactItem::Button { icon: Icon::Blocks, id: "ui.dialog.blocks", label: "Blocks Palette", tip: "Blocks Palette" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Reference",
                items: &[
                    RibbonItem::Large { icon: Icon::Attach, id: "xattach", label: "Attach", tip: "Attach DWG Reference" },
                    RibbonItem::Large { icon: Icon::Properties, id: "xref", label: "Xrefs", tip: "External References Manager" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Switcher, id: "xopen", label: "Open Xref", tip: "Open Referenced Drawing" },
                        CompactItem::Button { icon: Icon::BlockCreate, id: "xbind", label: "Bind", tip: "Bind Xref Symbols" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Import",
                items: &[RibbonItem::Large { icon: Icon::Import, id: "import", label: "Import", tip: "Import Vector or Drawing" }],
                more: &[],
            },
        ],
    },
    RibbonTab {
        id: "Annotate",
        label: "Annotate",
        panels: &[
            RibbonPanel {
                title: "Text",
                items: &[
                    RibbonItem::Large { icon: Icon::MText, id: "mtext", label: "Multiline", tip: "Multiline Text" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Text, id: "text", label: "Single Line", tip: "Single Line Text" },
                        CompactItem::Button { icon: Icon::Text, id: "style", label: "Text Style", tip: "Text Style Manager" },
                        CompactItem::Button { icon: Icon::Search, id: "ui.dialog.commands", label: "Commands", tip: "Command Reference" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Dimensions",
                items: &[
                    RibbonItem::Large { icon: Icon::DimLinear, id: "dimlinear", label: "Dimension", tip: "Linear Dimension" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::DimAligned, id: "dimaligned", label: "Aligned", tip: "Aligned Dimension" },
                        CompactItem::Button { icon: Icon::DimAngular, id: "dimangular", label: "Angular", tip: "Angular Dimension" },
                        CompactItem::Button { icon: Icon::DimRadius, id: "dimradius", label: "Radius", tip: "Radius Dimension" },
                    ]),
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::DimDiameter, id: "dimdiameter", label: "Diameter", tip: "Diameter Dimension" },
                        CompactItem::Button { icon: Icon::DimArc, id: "dimarc", label: "Arc Length", tip: "Arc Length Dimension" },
                        CompactItem::Button { icon: Icon::DimContinue, id: "dimcontinue", label: "Continue", tip: "Continue Dimension" },
                    ]),
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::DimBaseline, id: "dimbaseline", label: "Baseline", tip: "Baseline Dimension" },
                        CompactItem::Button { icon: Icon::DimOrdinate, id: "dimordinate", label: "Ordinate", tip: "Ordinate Dimension" },
                        CompactItem::Button { icon: Icon::QDim, id: "qdim", label: "Quick Dim", tip: "Quick Dimension" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Leaders",
                items: &[
                    RibbonItem::Large { icon: Icon::MLeader, id: "mleader", label: "Leader", tip: "Multileader" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::AddLeader, id: "mleaderedit.add", label: "Add Leader", tip: "Add Leader" },
                        CompactItem::Button { icon: Icon::RemoveLeader, id: "mleaderedit.remove", label: "Remove Leader", tip: "Remove Leader" },
                        CompactItem::Button { icon: Icon::AlignLeaders, id: "mleaderalign", label: "Align", tip: "Align Leaders" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Tables",
                items: &[
                    RibbonItem::Large { icon: Icon::Table, id: "table", label: "Table", tip: "Table" },
                    RibbonItem::CompactColumn(&[CompactItem::Button {
                        icon: Icon::Table,
                        id: "tablestyle",
                        label: "Table Style",
                        tip: "Table Style Manager",
                    }]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Markup",
                items: &[
                    RibbonItem::Large { icon: Icon::RevCloud, id: "revcloud", label: "Rev Cloud", tip: "Revision Cloud" },
                    RibbonItem::CompactColumn(&[CompactItem::Button { icon: Icon::Wipeout, id: "wipeout", label: "Wipeout", tip: "Wipeout" }]),
                ],
                more: &[],
            },
        ],
    },
    RibbonTab {
        id: "Parametric",
        label: "Parametric",
        panels: &[
            RibbonPanel {
                title: "Geometric",
                items: &[
                    RibbonItem::Large { icon: Icon::Constraint, id: "autoconstrain", label: "Auto", tip: "Auto Constrain" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Coincident, id: "gccoincident", label: "Coincident", tip: "Coincident" },
                        CompactItem::Button { icon: Icon::Parallel, id: "gcparallel", label: "Parallel", tip: "Parallel" },
                        CompactItem::Button { icon: Icon::Perpendicular, id: "gcperpendicular", label: "Perpendicular", tip: "Perpendicular" },
                    ]),
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Horizontal, id: "gchorizontal", label: "Horizontal", tip: "Horizontal" },
                        CompactItem::Button { icon: Icon::Vertical, id: "gcvertical", label: "Vertical", tip: "Vertical" },
                        CompactItem::Button { icon: Icon::Tangent, id: "gctangent", label: "Tangent", tip: "Tangent" },
                    ]),
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Concentric, id: "gcconcentric", label: "Concentric", tip: "Concentric" },
                        CompactItem::Button { icon: Icon::Equal, id: "gcequal", label: "Equal", tip: "Equal" },
                        CompactItem::Button { icon: Icon::Fix, id: "gcfix", label: "Fix", tip: "Fix" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Dimensional",
                items: &[
                    RibbonItem::Large { icon: Icon::DimLinear, id: "dclinear", label: "Linear", tip: "Linear Constraint" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::DimAligned, id: "dcaligned", label: "Aligned", tip: "Aligned Constraint" },
                        CompactItem::Button { icon: Icon::DimRadius, id: "dcradius", label: "Radius", tip: "Radius Constraint" },
                        CompactItem::Button { icon: Icon::DimDiameter, id: "dcdiameter", label: "Diameter", tip: "Diameter Constraint" },
                    ]),
                    RibbonItem::CompactColumn(&[CompactItem::Button {
                        icon: Icon::DimAngular,
                        id: "dcangular",
                        label: "Angular",
                        tip: "Angular Constraint",
                    }]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Manage",
                items: &[
                    RibbonItem::Large { icon: Icon::DelConstraint, id: "delconstraint", label: "Delete", tip: "Delete Constraints" },
                    RibbonItem::Large { icon: Icon::Parameters, id: "parameters", label: "Parameters", tip: "Parameters Manager" },
                ],
                more: &[],
            },
        ],
    },
    RibbonTab {
        id: "View",
        label: "View",
        panels: &[
            RibbonPanel {
                title: "Navigate",
                items: &[
                    RibbonItem::Large { icon: Icon::Pan, id: "pan", label: "Pan", tip: "Pan Realtime (P)" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::ZoomExtents, id: "zoom.extents", label: "Extents", tip: "Zoom Extents (Z E)" },
                        CompactItem::Button { icon: Icon::ZoomWindow, id: "zoom.window", label: "Window", tip: "Zoom Window (Z W)" },
                        CompactItem::Button { icon: Icon::Measure, id: "zoom.previous", label: "Previous", tip: "Zoom Previous (Z P)" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Views",
                items: &[
                    RibbonItem::Large { icon: Icon::Orbit, id: "ui.toggle.viewcube", label: "ViewCube", tip: "ViewCube Navigation" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Switcher, id: "ui.toggle.ucsicon", label: "UCS Icon", tip: "UCS Icon On/Off" },
                        CompactItem::Button { icon: Icon::Switcher, id: "view", label: "Named Views", tip: "Named Views Manager" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Palettes",
                items: &[
                    RibbonItem::Large { icon: Icon::Properties, id: "ui.panel.properties", label: "Properties", tip: "Properties Inspector" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Layers, id: "ui.dialog.layers", label: "Layers", tip: "Layers Palette" },
                        CompactItem::Button { icon: Icon::Gear, id: "ui.toggle.quickcalc", label: "QuickCalc", tip: "QuickCalc Calculator" },
                        CompactItem::Button { icon: Icon::Blocks, id: "ui.dialog.blocks", label: "Blocks", tip: "Blocks Palette" },
                    ]),
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Menu, id: "ui.toggle.toolsets", label: "Tool Sets", tip: "Tool Sets Panel" },
                        CompactItem::Button { icon: Icon::Text, id: "ui.toggle.cmdline", label: "Command Line", tip: "Command Line" },
                    ]),
                ],
                more: &[],
            },
        ],
    },
    RibbonTab {
        id: "Manage",
        label: "Manage",
        panels: &[
            RibbonPanel {
                title: "Customization",
                items: &[
                    RibbonItem::Large { icon: Icon::Gear, id: "options", label: "Options", tip: "Options Dialog (OPTIONS)" },
                    RibbonItem::CompactColumn(&[CompactItem::Button {
                        icon: Icon::Gear,
                        id: "dsettings",
                        label: "Drafting Settings",
                        tip: "Drafting Settings (DSETTINGS)",
                    }]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Standards",
                items: &[
                    RibbonItem::Large { icon: Icon::Help, id: "audit", label: "Audit", tip: "Audit Drawing (AUDIT)" },
                    RibbonItem::CompactColumn(&[CompactItem::Button {
                        icon: Icon::Erase,
                        id: "purge",
                        label: "Purge",
                        tip: "Purge Unused Named Objects",
                    }]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Applications",
                items: &[RibbonItem::Large { icon: Icon::Parameters, id: "appload", label: "Load App", tip: "Load AutoLISP Applications (APPLOAD)" }],
                more: &[],
            },
        ],
    },
    RibbonTab {
        id: "Output",
        label: "Output",
        panels: &[
            RibbonPanel {
                title: "Plot",
                items: &[
                    RibbonItem::Large { icon: Icon::Plot, id: "plot", label: "Plot", tip: "Plot / Print Drawing (Cmd+P)" },
                    RibbonItem::CompactColumn(&[
                        CompactItem::Button { icon: Icon::Preview, id: "preview", label: "Preview", tip: "Plot Preview" },
                        CompactItem::Button { icon: Icon::PageSetup, id: "pagesetup", label: "Page Setup", tip: "Page Setup Manager" },
                        CompactItem::Button { icon: Icon::Publish, id: "publish", label: "Publish", tip: "Batch Publish" },
                    ]),
                ],
                more: &[],
            },
            RibbonPanel {
                title: "Export",
                items: &[
                    RibbonItem::Large { icon: Icon::Export, id: "exportpdf", label: "Export PDF", tip: "Export to PDF" },
                    RibbonItem::CompactColumn(&[CompactItem::Button {
                        icon: Icon::Export,
                        id: "export",
                        label: "Export",
                        tip: "Export Other Formats",
                    }]),
                ],
                more: &[],
            },
        ],
    },
];

pub fn is_command_available(session: &Session, cmd: &str) -> bool {
    if crate::menus::is_ui_command(cmd) {
        return true;
    }
    if let Some(spec) = cadcraft_engine::find_command(cmd) { (spec.enabled)(session).is_ok() } else { false }
}

const TAB_HEIGHT: f32 = 26.0;
const PANELS_HEIGHT: f32 = 90.0;

/// Render the Ribbon top panel.
pub fn show(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let collapsed = app.ui.ribbon_collapsed;
    let total_h = TAB_HEIGHT + if collapsed { 0.0 } else { PANELS_HEIGHT };
    let mut clicked_cmd: Option<&str> = None;

    egui::Panel::top("cc_ribbon").exact_size(total_h).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let full_r = ui.max_rect();

        // 1. Tab header bar (height 26 px)
        let tab_row_r = Rect::from_min_size(full_r.min, vec2(full_r.width(), TAB_HEIGHT));
        ui.scope_builder(egui::UiBuilder::new().max_rect(tab_row_r), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                let active_tab_id = app.ui.ribbon_tab.clone();

                for tab in TABS {
                    let active = active_tab_id == tab.id;
                    let translated = crate::i18n::t(tab.label);
                    let galley = ui.painter().layout_no_wrap(
                        translated.to_string(),
                        egui::FontId::proportional(12.5),
                        if active { t.text } else { t.text_dim },
                    );
                    let tab_w = (galley.size().x + 20.0).max(54.0);
                    let (tr, resp) = ui.allocate_exact_size(vec2(tab_w, TAB_HEIGHT), Sense::click());

                    if active {
                        ui.painter().rect_filled(tr, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, t.panel);
                        ui.painter().vline(tr.left(), tr.y_range(), Stroke::new(1.0, t.border));
                        ui.painter().vline(tr.right(), tr.y_range(), Stroke::new(1.0, t.border));
                    } else if resp.hovered() {
                        ui.painter().rect_filled(tr, egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, t.control_hover.gamma_multiply(0.4));
                    }

                    let text_pos = pos2(tr.center().x - galley.size().x / 2.0, tr.center().y - galley.size().y / 2.0);
                    ui.painter().galley(text_pos, galley, if active { t.text } else { t.text_dim });

                    icons::describe_control(ui, &resp, tab.label, Some(active), true);
                    if resp.clicked() {
                        app.ui.ribbon_tab = tab.id.to_string();
                        if collapsed {
                            app.ui.ribbon_collapsed = false;
                        }
                    }
                }

                // Right side chevron to collapse / expand ribbon panels
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_r = Rect::from_center_size(pos2(ui.available_rect_before_wrap().right() - 14.0, tab_row_r.center().y), vec2(18.0, 18.0));
                    let resp = ui.interact(btn_r, ui.id().with("ribbon_collapse_btn"), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(btn_r, 3.0, t.control_hover.gamma_multiply(0.5));
                    }
                    icons::paint(ui.painter(), btn_r.shrink(2.0), if collapsed { Icon::ChevronDown } else { Icon::ChevronUp }, false);
                    let tip = if collapsed { crate::tl!("Expand the Ribbon") } else { crate::tl!("Minimize the Ribbon") };
                    if resp.on_hover_text(tip).clicked() {
                        app.ui.ribbon_collapsed = !app.ui.ribbon_collapsed;
                    }
                });
            });
        });

        // 2. Ribbon Panels Body (height 90 px)
        if !collapsed {
            let panels_r = Rect::from_min_size(pos2(full_r.left(), full_r.top() + TAB_HEIGHT), vec2(full_r.width(), PANELS_HEIGHT));

            ui.scope_builder(egui::UiBuilder::new().max_rect(panels_r), |ui| {
                ui.painter().rect_filled(panels_r, 0.0, t.panel);
                ui.painter().hline(panels_r.x_range(), panels_r.bottom() - 1.0, Stroke::new(1.0, t.border));

                egui::ScrollArea::horizontal().id_salt("cc_ribbon_panels_scroll").auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    ui.horizontal(|ui| {
                        let active_tab = TABS.iter().find(|tab| tab.id == app.ui.ribbon_tab).or_else(|| TABS.first());

                        if let Some(active_tab) = active_tab {
                            for panel in active_tab.panels {
                                render_panel(ui, app, panel, &mut clicked_cmd);
                            }
                        }
                    });
                });
            });
        }
    });

    if let Some(cmd) = clicked_cmd {
        crate::menus::activate(app, cmd);
    }
}

fn render_panel(ui: &mut egui::Ui, app: &mut CadApp, panel: &RibbonPanel, clicked: &mut Option<&'static str>) {
    let t = Tokens::get();

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

        // 1. Panel Tools Area (Height ~68px)
        let tools_resp = ui.allocate_ui(vec2(0.0, 68.0), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
                ui.add_space(3.0);

                for item in panel.items {
                    match item {
                        RibbonItem::Large { icon, id, label, tip } => {
                            let available = is_command_available(&app.session, id);
                            if render_large_button(ui, *icon, id, label, tip, available) {
                                *clicked = Some(*id);
                            }
                        }
                        RibbonItem::SplitButton { icon, id, label, tip, menu } => {
                            let available = is_command_available(&app.session, id);
                            if let Some(cmd) = render_split_button(ui, *icon, id, label, tip, menu, available) {
                                *clicked = Some(cmd);
                            }
                        }
                        RibbonItem::LargeDropdown { icon, id, label, tip, menu } => {
                            let available = is_command_available(&app.session, id);
                            if let Some(selected) = render_large_dropdown(ui, *icon, id, label, tip, menu, available) {
                                *clicked = Some(selected);
                            }
                        }
                        RibbonItem::MediumGrid { cols } => {
                            render_medium_grid(ui, app, cols, clicked);
                        }
                        RibbonItem::CompactColumn(compact_items) => {
                            render_compact_column(ui, app, compact_items, clicked);
                        }
                        RibbonItem::IconColumn(icon_items) => {
                            render_icon_column(ui, app, icon_items, clicked);
                        }
                        RibbonItem::LayersControl => {
                            render_layers_control(ui, app, clicked);
                        }
                        RibbonItem::PropertiesControl => {
                            render_properties_control(ui, app);
                        }
                    }
                }
                ui.add_space(3.0);
            });
        });

        // 2. Panel Title (Height 18px)
        let panel_width = tools_resp.response.rect.width().max(36.0);
        let (title_r, resp) = ui.allocate_exact_size(vec2(panel_width, 18.0), Sense::click());
        let title_text = crate::i18n::t(panel.title);
        let galley = ui.painter().layout_no_wrap(title_text.to_string(), egui::FontId::proportional(10.5), t.text_dim);

        let has_more = !panel.more.is_empty();
        let total_w = galley.size().x + if has_more { 10.0 } else { 0.0 };
        let title_pos = pos2(title_r.center().x - total_w / 2.0, title_r.center().y - galley.size().y / 2.0);
        ui.painter().galley(title_pos, galley, t.text_dim);

        if has_more {
            let chev_pos = pos2(title_r.center().x + total_w / 2.0 - 3.0, title_r.center().y);
            icons::paint(ui.painter(), Rect::from_center_size(chev_pos, vec2(7.0, 7.0)), Icon::ChevronDown, false);

            if resp.hovered() {
                ui.painter().rect_filled(title_r, 2.0, t.control_hover.gamma_multiply(0.4));
            }

            egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                for (cmd, m_label, m_icon) in panel.more {
                    ui.horizontal(|ui| {
                        let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        icons::paint(ui.painter(), ir, *m_icon, false);
                        let m_btn = ui.button(crate::i18n::t(m_label));
                        if m_btn.clicked() {
                            *clicked = Some(*cmd);
                            ui.close();
                        }
                    });
                }
            });
        }
    });

    // Panel vertical separator
    let (sep_r, _) = ui.allocate_exact_size(vec2(6.0, PANELS_HEIGHT), Sense::hover());
    let sep_x = sep_r.center().x;
    ui.painter().vline(sep_x, (sep_r.top() + 8.0)..=(sep_r.bottom() - 8.0), Stroke::new(1.0, t.border));
}

fn render_large_button(ui: &mut egui::Ui, icon: Icon, id: &'static str, label: &'static str, tip: &'static str, enabled: bool) -> bool {
    let t = Tokens::get();
    let btn_size = vec2(52.0, 64.0);
    let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());

    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
    }
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover);
    }

    // Paint Icon (24x24)
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 20.0), vec2(24.0, 24.0));
    icons::paint(ui.painter(), icon_rect, icon, !enabled);

    // Paint Label
    let translated = crate::i18n::t(label);
    let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), if enabled { t.text } else { t.text_dim });
    let label_pos = pos2(rect.center().x - galley.size().x / 2.0, rect.top() + 42.0);
    ui.painter().galley(label_pos, galley, if enabled { t.text } else { t.text_dim });

    icons::describe_control(ui, &resp, tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
    let resp = resp.on_hover_text(full_tip);

    enabled && resp.clicked()
}

fn command_tooltip(cmd: &str) -> Option<&'static str> {
    match cmd {
        "circle" => Some("Creates a circle using a center point and a radius (CIRCLE)"),
        "circle.cd" => Some("Creates a circle using a center point and a diameter (CIRCLE)"),
        "circle.2p" => Some("Creates a circle using two endpoints of the diameter (CIRCLE)"),
        "circle.3p" => Some("Creates a circle using three points on the circumference (CIRCLE)"),
        "circle.ttr" => Some("Creates a circle tangent to two objects with a specified radius (CIRCLE)"),
        "circle.ttt" => Some("Creates a circle tangent to three objects (CIRCLE)"),
        _ => None,
    }
}

fn render_split_button(
    ui: &mut egui::Ui,
    icon: Icon,
    id: &'static str,
    label: &'static str,
    tip: &'static str,
    menu: &'static [(&'static str, &'static str, Icon)],
    enabled: bool,
) -> Option<&'static str> {
    let t = Tokens::get();
    let btn_size = vec2(52.0, 64.0);
    let (rect, _) = ui.allocate_exact_size(btn_size, Sense::hover());
    let mut selected_cmd = None;

    // Split rect into top primary action area (height 48px) and bottom dropdown arrow area (height 16px)
    let top_rect = Rect::from_min_max(rect.min, pos2(rect.max.x, rect.top() + 48.0));
    let bot_rect = Rect::from_min_max(pos2(rect.min.x, rect.top() + 48.0), rect.max);

    let top_resp = ui.interact(top_rect, ui.id().with(("sb_top", id)), Sense::click());
    let bot_resp = ui.interact(bot_rect, ui.id().with(("sb_bot", id)), Sense::click());

    if enabled {
        if top_resp.hovered() {
            ui.painter().rect_filled(top_rect, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, t.control_hover.gamma_multiply(0.6));
        }
        if top_resp.is_pointer_button_down_on() {
            ui.painter().rect_filled(top_rect, egui::CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, t.control_hover);
        }
        if bot_resp.hovered() {
            ui.painter().rect_filled(bot_rect, egui::CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 }, t.control_hover.gamma_multiply(0.6));
        }
    }

    // Paint Icon (22x22)
    let icon_rect = Rect::from_center_size(pos2(top_rect.center().x, top_rect.top() + 17.0), vec2(22.0, 22.0));
    icons::paint(ui.painter(), icon_rect, icon, !enabled);

    // Paint Label
    let translated = crate::i18n::t(label);
    let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), if enabled { t.text } else { t.text_dim });
    let label_pos = pos2(top_rect.center().x - galley.size().x / 2.0, top_rect.top() + 34.0);
    ui.painter().galley(label_pos, galley, if enabled { t.text } else { t.text_dim });

    // Paint small ChevronDown in bottom area
    let chev_rect = Rect::from_center_size(pos2(bot_rect.center().x, bot_rect.center().y), vec2(8.0, 8.0));
    icons::paint(ui.painter(), chev_rect, Icon::ChevronDown, !enabled);

    // Subtle divider line between top and bottom
    ui.painter().hline((rect.left() + 4.0)..=(rect.right() - 4.0), top_rect.bottom(), Stroke::new(1.0, t.border.gamma_multiply(0.5)));

    icons::describe_control(ui, &top_resp, tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
    let top_resp = top_resp.on_hover_text(full_tip);

    icons::describe_control(ui, &bot_resp, &format!("{tip} variants"), None, false);
    let bot_resp = bot_resp.on_hover_text(format!("{} variants", crate::i18n::t(label)));

    if enabled {
        if top_resp.clicked() {
            selected_cmd = Some(id);
        }

        egui::Popup::from_toggle_button_response(&bot_resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for (cmd, m_label, m_icon) in menu {
                ui.horizontal(|ui| {
                    let tip = command_tooltip(cmd).map(crate::i18n::t).unwrap_or_else(|| crate::i18n::t(m_label));
                    let (ir, ir_resp) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::click());
                    icons::paint(ui.painter(), ir, *m_icon, false);
                    let ir_resp = ir_resp.on_hover_text(tip);
                    let m_btn = ui.button(crate::i18n::t(m_label)).on_hover_text(tip);
                    if m_btn.clicked() || ir_resp.clicked() {
                        selected_cmd = Some(*cmd);
                        ui.close();
                    }
                });
            }
        });
    }

    selected_cmd
}

fn render_large_dropdown(
    ui: &mut egui::Ui,
    icon: Icon,
    id: &'static str,
    label: &'static str,
    tip: &'static str,
    menu: &'static [(&'static str, &'static str, Icon)],
    enabled: bool,
) -> Option<&'static str> {
    let t = Tokens::get();
    let btn_size = vec2(52.0, 64.0);
    let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());
    let mut selected_cmd = None;

    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
    }

    // Paint Icon
    let icon_rect = Rect::from_center_size(pos2(rect.center().x, rect.top() + 19.0), vec2(24.0, 24.0));
    icons::paint(ui.painter(), icon_rect, icon, !enabled);

    // Paint Label + Chevron
    let translated = crate::i18n::t(label);
    let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), if enabled { t.text } else { t.text_dim });
    let total_w = galley.size().x + 9.0;
    let label_pos = pos2(rect.center().x - total_w / 2.0, rect.top() + 42.0);
    ui.painter().galley(label_pos, galley, if enabled { t.text } else { t.text_dim });

    // Tiny Chevron Down
    let chev_rect = Rect::from_center_size(pos2(rect.center().x + total_w / 2.0 - 3.0, rect.top() + 47.0), vec2(7.0, 7.0));
    icons::paint(ui.painter(), chev_rect, Icon::ChevronDown, !enabled);

    icons::describe_control(ui, &resp, tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
    let resp = resp.on_hover_text(full_tip);

    if enabled {
        egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for (cmd, m_label, m_icon) in menu {
                ui.horizontal(|ui| {
                    let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    icons::paint(ui.painter(), ir, *m_icon, false);
                    let m_btn = ui.button(crate::i18n::t(m_label));
                    if m_btn.clicked() {
                        selected_cmd = Some(*cmd);
                        ui.close();
                    }
                });
            }
        });

        if resp.clicked() {
            selected_cmd = Some(id);
        }
    }

    selected_cmd
}

fn render_medium_grid(ui: &mut egui::Ui, app: &CadApp, cols: &'static [&'static [MediumItem]], clicked: &mut Option<&'static str>) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
        for col in cols {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
                ui.add_space(2.0);
                for item in *col {
                    match item {
                        MediumItem::Button { icon, id, label, tip } => {
                            let available = is_command_available(&app.session, id);
                            if render_medium_button(ui, *icon, id, label, tip, available) {
                                *clicked = Some(*id);
                            }
                        }
                        MediumItem::Dropdown { icon, id, label, tip, menu } => {
                            let available = is_command_available(&app.session, id);
                            if let Some(cmd) = render_medium_split_button(ui, *icon, id, label, tip, menu, available) {
                                *clicked = Some(cmd);
                            }
                        }
                    }
                }
            });
        }
    });
}

fn render_medium_button(ui: &mut egui::Ui, icon: Icon, id: &'static str, label: &'static str, tip: &'static str, enabled: bool) -> bool {
    let t = Tokens::get();
    let btn_size = vec2(92.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());

    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
    }
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover);
    }

    // Icon (16x16) on the left
    let icon_rect = Rect::from_center_size(pos2(rect.left() + 10.0, rect.center().y), vec2(15.0, 15.0));
    icons::paint(ui.painter(), icon_rect, icon, !enabled);

    // Label on the right
    let translated = crate::i18n::t(label);
    let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), if enabled { t.text } else { t.text_dim });
    let label_pos = pos2(rect.left() + 20.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(label_pos, galley, if enabled { t.text } else { t.text_dim });

    icons::describe_control(ui, &resp, tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
    let resp = resp.on_hover_text(full_tip);

    enabled && resp.clicked()
}

fn render_medium_split_button(
    ui: &mut egui::Ui,
    default_icon: Icon,
    default_id: &'static str,
    default_label: &'static str,
    default_tip: &'static str,
    menu: &'static [(&'static str, &'static str, Icon)],
    enabled: bool,
) -> Option<&'static str> {
    let t = Tokens::get();
    let btn_size = vec2(92.0, 20.0);
    let (rect, _) = ui.allocate_exact_size(btn_size, Sense::hover());
    let mut selected_cmd = None;

    // Active command tracking: defaults to default_id, switches when a variant is selected from dropdown
    let active_cmd: &'static str = ui.data_mut(|d| d.get_temp(ui.id().with(("med_sb_active", default_id)))).unwrap_or(default_id);

    let (cur_icon, cur_label, cur_tip) = if let Some((cmd, m_label, m_icon)) = menu.iter().find(|(cmd, _, _)| *cmd == active_cmd) {
        let label = if *cmd == "arrayrect" { default_label } else { *m_label };
        let tip = if *cmd == "arrayrect" { "Rectangular Array" } else { default_tip };
        (*m_icon, label, tip)
    } else {
        (default_icon, default_label, default_tip)
    };

    // Main action zone on left (78px), arrow zone on right (14px)
    let arrow_width = 14.0;
    let main_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - arrow_width, rect.max.y));
    let arrow_rect = Rect::from_min_max(pos2(rect.max.x - arrow_width, rect.min.y), rect.max);

    let main_resp = ui.interact(main_rect, ui.id().with(("med_sb_main", default_id)), Sense::click());
    let arrow_resp = ui.interact(arrow_rect, ui.id().with(("med_sb_arrow", default_id)), Sense::click());

    if enabled {
        if main_resp.hovered() {
            ui.painter().rect_filled(main_rect, egui::CornerRadius { nw: 3, ne: 0, sw: 3, se: 0 }, t.control_hover.gamma_multiply(0.6));
        }
        if main_resp.is_pointer_button_down_on() {
            ui.painter().rect_filled(main_rect, egui::CornerRadius { nw: 3, ne: 0, sw: 3, se: 0 }, t.control_hover);
        }
        if arrow_resp.hovered() {
            ui.painter().rect_filled(arrow_rect, egui::CornerRadius { nw: 0, ne: 3, sw: 0, se: 3 }, t.control_hover.gamma_multiply(0.6));
        }
        if arrow_resp.is_pointer_button_down_on() {
            ui.painter().rect_filled(arrow_rect, egui::CornerRadius { nw: 0, ne: 3, sw: 0, se: 3 }, t.control_hover);
        }
    }

    // Paint main icon (15x15)
    let icon_rect = Rect::from_center_size(pos2(main_rect.left() + 10.0, main_rect.center().y), vec2(15.0, 15.0));
    icons::paint(ui.painter(), icon_rect, cur_icon, !enabled);

    // Paint main label (within main_rect, never overlaps arrow)
    let translated = crate::i18n::t(cur_label);
    let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), if enabled { t.text } else { t.text_dim });
    let label_pos = pos2(main_rect.left() + 20.0, main_rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(label_pos, galley, if enabled { t.text } else { t.text_dim });

    // Subtle divider line between main and arrow
    ui.painter().vline(main_rect.right(), (rect.top() + 2.0)..=(rect.bottom() - 2.0), Stroke::new(1.0, t.border.gamma_multiply(0.5)));

    // Paint ChevronDown in arrow area
    let chev_rect = Rect::from_center_size(arrow_rect.center(), vec2(6.0, 6.0));
    icons::paint(ui.painter(), chev_rect, Icon::ChevronDown, !enabled);

    icons::describe_control(ui, &main_resp, cur_tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(cur_tip), active_cmd);
    let main_resp = main_resp.on_hover_text(full_tip);

    icons::describe_control(ui, &arrow_resp, &format!("{cur_tip} options"), None, false);
    let arrow_resp = arrow_resp.on_hover_text(format!("{} options", crate::i18n::t(cur_label)));

    if enabled {
        if main_resp.clicked() {
            selected_cmd = Some(active_cmd);
        }

        egui::Popup::from_toggle_button_response(&arrow_resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for (cmd, m_label, m_icon) in menu {
                ui.horizontal(|ui| {
                    let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    icons::paint(ui.painter(), ir, *m_icon, false);
                    let m_btn = ui.button(crate::i18n::t(m_label));
                    if m_btn.clicked() {
                        ui.data_mut(|d| {
                            d.insert_temp(ui.id().with(("med_sb_active", default_id)), *cmd);
                        });
                        selected_cmd = Some(*cmd);
                        ui.close();
                    }
                });
            }
        });
    }

    selected_cmd
}

fn render_icon_column(ui: &mut egui::Ui, app: &CadApp, items: &'static [IconItem], clicked: &mut Option<&'static str>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        ui.add_space(2.0);
        for item in items {
            let available = is_command_available(&app.session, item.id);
            if render_compact_icon_button(ui, item.icon, item.id, item.tip, available) {
                *clicked = Some(item.id);
            }
        }
    });
}

fn render_compact_icon_button(ui: &mut egui::Ui, icon: Icon, id: &'static str, tip: &'static str, enabled: bool) -> bool {
    let t = Tokens::get();
    let btn_size = vec2(22.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());

    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
    }
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover);
    }

    let icon_rect = Rect::from_center_size(rect.center(), vec2(15.0, 15.0));
    icons::paint(ui.painter(), icon_rect, icon, !enabled);

    icons::describe_control(ui, &resp, tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
    let resp = resp.on_hover_text(full_tip);

    enabled && resp.clicked()
}

fn render_compact_icon_button_sized(ui: &mut egui::Ui, icon: Icon, id: &'static str, tip: &'static str, enabled: bool, width: f32) -> bool {
    let t = Tokens::get();
    let btn_size = vec2(width, 20.0);
    let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());

    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
    }
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover);
    }

    let icon_rect = Rect::from_center_size(rect.center(), vec2(15.0, 15.0));
    icons::paint(ui.painter(), icon_rect, icon, !enabled);

    icons::describe_control(ui, &resp, tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
    let resp = resp.on_hover_text(full_tip);

    enabled && resp.clicked()
}

fn render_compact_column(ui: &mut egui::Ui, app: &CadApp, items: &'static [CompactItem], clicked: &mut Option<&'static str>) {
    let all_icon_only = items.iter().all(|item| match item {
        CompactItem::Button { label, .. } | CompactItem::Dropdown { label, .. } => label.is_empty(),
        CompactItem::IconOnly { .. } => true,
    });

    let col_w = if all_icon_only {
        34.0
    } else {
        let mut max_w: f32 = 68.0;
        for item in items {
            match item {
                CompactItem::Button { label, .. } if !label.is_empty() => {
                    let translated = crate::i18n::t(label);
                    let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), egui::Color32::WHITE);
                    max_w = max_w.max(galley.size().x + 28.0);
                }
                CompactItem::Dropdown { label, .. } if !label.is_empty() => {
                    let translated = crate::i18n::t(label);
                    let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), egui::Color32::WHITE);
                    max_w = max_w.max(galley.size().x + 36.0);
                }
                _ => {}
            }
        }
        max_w
    };

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        ui.add_space(2.0);

        for item in items {
            match item {
                CompactItem::Button { icon, id, label, tip } => {
                    let available = is_command_available(&app.session, id);
                    if render_compact_button(ui, *icon, id, label, tip, available, col_w) {
                        *clicked = Some(*id);
                    }
                }
                CompactItem::Dropdown { icon, id, label, tip, menu } => {
                    let available = is_command_available(&app.session, id);
                    if let Some(selected) = render_compact_dropdown(ui, *icon, id, label, tip, menu, available, col_w) {
                        *clicked = Some(selected);
                    }
                }
                CompactItem::IconOnly { icon, id, tip } => {
                    let available = is_command_available(&app.session, id);
                    if render_compact_icon_button_sized(ui, *icon, id, tip, available, col_w) {
                        *clicked = Some(*id);
                    }
                }
            }
        }
    });
}

fn render_compact_button(ui: &mut egui::Ui, icon: Icon, id: &'static str, label: &'static str, tip: &'static str, enabled: bool, width: f32) -> bool {
    let t = Tokens::get();
    let btn_size = vec2(width, 20.0);
    let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());

    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
    }
    if enabled && resp.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, 3.0, t.control_hover);
    }

    if label.is_empty() {
        // Icon only, centered
        let icon_rect = Rect::from_center_size(rect.center(), vec2(15.0, 15.0));
        icons::paint(ui.painter(), icon_rect, icon, !enabled);
    } else {
        // Icon (15x15) on the left
        let icon_rect = Rect::from_center_size(pos2(rect.left() + 11.0, rect.center().y), vec2(15.0, 15.0));
        icons::paint(ui.painter(), icon_rect, icon, !enabled);

        // Label on the right
        let translated = crate::i18n::t(label);
        let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), if enabled { t.text } else { t.text_dim });
        let label_pos = pos2(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(label_pos, galley, if enabled { t.text } else { t.text_dim });
    }

    icons::describe_control(ui, &resp, tip, None, false);
    let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
    let resp = resp.on_hover_text(full_tip);

    enabled && resp.clicked()
}

fn render_compact_dropdown(
    ui: &mut egui::Ui,
    icon: Icon,
    id: &'static str,
    label: &'static str,
    tip: &'static str,
    menu: &'static [(&'static str, &'static str, Icon)],
    enabled: bool,
    width: f32,
) -> Option<&'static str> {
    let t = Tokens::get();
    let btn_size = vec2(width, 20.0);
    let mut selected_cmd = None;

    if label.is_empty() {
        // Compact icon + arrow split dropdown (AutoCAD style for Draw panel: Rectangle, Hatch)
        let (rect, _) = ui.allocate_exact_size(btn_size, Sense::hover());
        let icon_rect = Rect::from_min_max(rect.min, pos2(rect.max.x - 12.0, rect.max.y));
        let arrow_rect = Rect::from_min_max(pos2(rect.max.x - 12.0, rect.min.y), rect.max);

        let icon_resp = ui.interact(icon_rect, ui.id().with(("cd_icon", id)), Sense::click());
        let arrow_resp = ui.interact(arrow_rect, ui.id().with(("cd_arr", id)), Sense::click());

        if enabled {
            if icon_resp.hovered() {
                ui.painter().rect_filled(icon_rect, egui::CornerRadius { nw: 3, ne: 0, sw: 3, se: 0 }, t.control_hover.gamma_multiply(0.6));
            }
            if icon_resp.is_pointer_button_down_on() {
                ui.painter().rect_filled(icon_rect, egui::CornerRadius { nw: 3, ne: 0, sw: 3, se: 0 }, t.control_hover);
            }
            if arrow_resp.hovered() {
                ui.painter().rect_filled(arrow_rect, egui::CornerRadius { nw: 0, ne: 3, sw: 0, se: 3 }, t.control_hover.gamma_multiply(0.6));
            }
            if arrow_resp.is_pointer_button_down_on() {
                ui.painter().rect_filled(arrow_rect, egui::CornerRadius { nw: 0, ne: 3, sw: 0, se: 3 }, t.control_hover);
            }
        }

        let ir = Rect::from_center_size(icon_rect.center(), vec2(15.0, 15.0));
        icons::paint(ui.painter(), ir, icon, !enabled);

        ui.painter().vline(icon_rect.right(), (rect.top() + 2.0)..=(rect.bottom() - 2.0), Stroke::new(1.0, t.border.gamma_multiply(0.5)));

        let chev_rect = Rect::from_center_size(arrow_rect.center(), vec2(6.0, 6.0));
        icons::paint(ui.painter(), chev_rect, Icon::ChevronDown, !enabled);

        icons::describe_control(ui, &icon_resp, tip, None, false);
        let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
        let icon_resp = icon_resp.on_hover_text(full_tip);

        icons::describe_control(ui, &arrow_resp, &format!("{tip} options"), None, false);
        let arrow_resp = arrow_resp.on_hover_text(format!("{} options", crate::i18n::t(tip)));

        if enabled {
            if icon_resp.clicked() {
                selected_cmd = Some(id);
            }

            egui::Popup::from_toggle_button_response(&arrow_resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                for (cmd, m_label, m_icon) in menu {
                    ui.horizontal(|ui| {
                        let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        icons::paint(ui.painter(), ir, *m_icon, false);
                        let m_btn = ui.button(crate::i18n::t(m_label));
                        if m_btn.clicked() {
                            selected_cmd = Some(*cmd);
                            ui.close();
                        }
                    });
                }
            });
        }
    } else {
        let (rect, resp) = ui.allocate_exact_size(btn_size, Sense::click());

        if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
        }
        if enabled && resp.is_pointer_button_down_on() {
            ui.painter().rect_filled(rect, 3.0, t.control_hover);
        }

        // Icon (15x15)
        let icon_rect = Rect::from_center_size(pos2(rect.left() + 11.0, rect.center().y), vec2(15.0, 15.0));
        icons::paint(ui.painter(), icon_rect, icon, !enabled);

        // Label
        let translated = crate::i18n::t(label);
        let galley = ui.painter().layout_no_wrap(translated.to_string(), egui::FontId::proportional(11.0), if enabled { t.text } else { t.text_dim });
        let label_pos = pos2(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(label_pos, galley, if enabled { t.text } else { t.text_dim });

        // Chevron
        let chev_rect = Rect::from_center_size(pos2(rect.right() - 8.0, rect.center().y), vec2(6.0, 6.0));
        icons::paint(ui.painter(), chev_rect, Icon::ChevronDown, !enabled);

        icons::describe_control(ui, &resp, tip, None, false);
        let full_tip = format!("{} ({})", crate::i18n::t(tip), id);
        let resp = resp.on_hover_text(full_tip);

        if enabled {
            egui::Popup::from_toggle_button_response(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                for (cmd, m_label, m_icon) in menu {
                    ui.horizontal(|ui| {
                        let (ir, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                        icons::paint(ui.painter(), ir, *m_icon, false);
                        let m_btn = ui.button(crate::i18n::t(m_label));
                        if m_btn.clicked() {
                            selected_cmd = Some(*cmd);
                            ui.close();
                        }
                    });
                }
            });

            if resp.clicked() {
                selected_cmd = Some(id);
            }
        }
    }

    selected_cmd
}

fn render_layers_control(ui: &mut egui::Ui, app: &mut CadApp, clicked: &mut Option<&'static str>) {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else { return };
    let cur = d.header.str("CLAYER", "0");
    let layers: Vec<(String, bool, bool, bool, Color)> = d.layers.iter().map(|l| (l.name.clone(), l.on, l.frozen, l.locked, l.color)).collect();

    let sel = app.session.selection();
    let has_sel = !sel.is_empty();
    let sel_layers: Vec<String> = sel.iter().filter_map(|h| d.entity(*h)).map(|e| e.common.layer.clone()).collect();
    let displayed_layer_name = if has_sel {
        if sel_layers.is_empty() {
            cur.clone()
        } else if sel_layers.iter().all(|l| l == &sel_layers[0]) {
            sel_layers[0].clone()
        } else {
            crate::tl!("*VARIES*").to_string()
        }
    } else {
        cur.clone()
    };

    let cur_info = layers.iter().find(|l| l.0.eq_ignore_ascii_case(&displayed_layer_name));
    let color = cur_info.map(|l| l.4).unwrap_or(Color::Index(7));
    let is_on = cur_info.map(|l| l.1).unwrap_or(true);
    let is_frozen = cur_info.map(|l| l.2).unwrap_or(false);
    let is_locked = cur_info.map(|l| l.3).unwrap_or(false);

    let mut set_layer = None;
    let mut toggle_layer_prop: Option<(String, &'static str, bool)> = None;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        ui.add_space(2.0);

        // Top: Layer ComboBox (~160px wide, 22px high)
        let combo_w = 160.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(combo_w, 22.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 3.0, t.control_hover.gamma_multiply(0.6));
        } else {
            ui.painter().rect_filled(rect, 3.0, t.control);
        }

        // Draw Layer status mini-icons + swatch + name
        let mut x = rect.left() + 5.0;
        for icon in [
            if is_on { Icon::Bulb } else { Icon::BulbOff },
            if is_frozen { Icon::Snowflake } else { Icon::Sun },
            if is_locked { Icon::Lock } else { Icon::Unlock },
        ] {
            icons::paint(ui.painter(), Rect::from_min_size(pos2(x, rect.top() + 3.0), vec2(15.0, 15.0)), icon, false);
            x += 16.0;
        }

        // Swatch (10x10)
        let sr = Rect::from_min_size(pos2(x + 2.0, rect.top() + 6.0), vec2(10.0, 10.0));
        crate::palettes::swatch(ui.painter(), sr, color);
        x += 16.0;

        // Layer Name
        let name_w = (rect.right() - 16.0 - x).max(20.0);
        let mut job = egui::text::LayoutJob::simple_singleline(displayed_layer_name.clone(), egui::FontId::proportional(11.0), t.text);
        job.wrap = egui::text::TextWrapping { max_width: name_w, max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
        let galley = ui.painter().layout_job(job);
        let text_pos = pos2(x, rect.center().y - galley.size().y / 2.0);
        ui.painter().galley(text_pos, galley, t.text);

        // Dropdown Chevron
        icons::paint(ui.painter(), Rect::from_center_size(pos2(rect.right() - 9.0, rect.center().y), vec2(8.0, 8.0)), Icon::ChevronDown, false);

        icons::describe_control(ui, &resp, &format!("Layer: {displayed_layer_name}"), None, false);
        let resp = resp.on_hover_text(format!("{}: {displayed_layer_name}", crate::tl!("Current Layer")));

        // Layer picker popup
        egui::Popup::from_toggle_button_response(&resp).width(combo_w + 30.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(
            |ui| {
                egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                    for (name, on, frozen, locked, l_color) in &layers {
                        ui.horizontal(|ui| {
                            if icons::button(ui, if *on { Icon::Bulb } else { Icon::BulbOff }, 16.0, "On/Off", false).clicked() {
                                toggle_layer_prop = Some((name.clone(), "on", !*on));
                            }
                            if icons::button(ui, if *frozen { Icon::Snowflake } else { Icon::Sun }, 16.0, "Freeze/Thaw", false).clicked() {
                                toggle_layer_prop = Some((name.clone(), "frozen", !*frozen));
                            }
                            if icons::button(ui, if *locked { Icon::Lock } else { Icon::Unlock }, 16.0, "Lock/Unlock", false).clicked() {
                                toggle_layer_prop = Some((name.clone(), "locked", !*locked));
                            }
                            let (sr, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                            crate::palettes::swatch(ui.painter(), sr, *l_color);
                            let is_cur = name.eq_ignore_ascii_case(&displayed_layer_name);
                            if ui.selectable_label(is_cur, name).clicked() {
                                set_layer = Some(name.clone());
                                ui.close();
                            }
                        });
                    }
                });
            },
        );

        // Compact Layer Tools: Row 1 (Turn Off, Freeze, Lock)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
            let row1: &[(Icon, &'static str, &'static str)] = &[
                (Icon::LayerOff, "layoff", "Turn Layer Off"),
                (Icon::LayerFreeze, "layfrz", "Freeze Layer"),
                (Icon::LayerLock, "laylck", "Lock Layer"),
            ];

            for (icon, id, tip) in row1 {
                let available = is_command_available(&app.session, id);
                if render_compact_icon_button_sized(ui, *icon, id, tip, available, 26.0) {
                    *clicked = Some(*id);
                }
            }
        });

        // Compact Layer Tools: Row 2 (Isolate, Unisolate, Previous State)
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 0.0);
            let row2: &[(Icon, &'static str, &'static str)] = &[
                (Icon::LayerIso, "layiso", "Isolate Layer"),
                (Icon::LayerIso, "layuniso", "Unisolate Layer"),
                (Icon::LayerPrev, "layerp", "Previous Layer State"),
            ];

            for (icon, id, tip) in row2 {
                let available = is_command_available(&app.session, id);
                if render_compact_icon_button_sized(ui, *icon, id, tip, available, 26.0) {
                    *clicked = Some(*id);
                }
            }
        });
    });

    if let Some((name, prop, val)) = toggle_layer_prop {
        let _ = app.run("layer.set", serde_json::json!({ "name": name, prop: val }));
    }
    if let Some(name) = set_layer {
        if has_sel {
            let ids: Vec<String> = sel.iter().map(|h| h.hex()).collect();
            let _ = app.run("properties.set", serde_json::json!({ "handles": ids, "layer": name }));
        } else {
            let _ = app.run("layer.current", serde_json::json!({ "name": name }));
        }
    }
}

fn render_properties_control(ui: &mut egui::Ui, app: &mut CadApp) {
    let t = Tokens::get();
    let Ok(d) = app.session.doc() else { return };
    let h = &d.header;
    let sel = app.session.selection();
    let has_sel = !sel.is_empty();
    let ids: Vec<String> = sel.iter().map(|h| h.hex()).collect();

    // 1. Color
    let color_str = if has_sel {
        let colors: Vec<_> = sel.iter().filter_map(|h| d.entity(*h)).map(|e| e.common.color).collect();
        if colors.is_empty() {
            "ByLayer".into()
        } else if colors.iter().all(|c| *c == colors[0]) {
            colors[0].name()
        } else {
            crate::tl!("*VARIES*").to_string()
        }
    } else {
        cadcraft_color::Color::from_aci(h.i64("CECOLOR", 256) as i16).name()
    };

    // 2. Lineweight
    let lw_str = if has_sel {
        let lws: Vec<_> = sel.iter().filter_map(|h| d.entity(*h)).map(|e| e.common.lineweight).collect();
        if lws.is_empty() {
            "ByLayer".into()
        } else if lws.iter().all(|l| *l == lws[0]) {
            lws[0].name()
        } else {
            crate::tl!("*VARIES*").to_string()
        }
    } else {
        cadcraft_doc::Lineweight::from_dxf(h.i64("CELWEIGHT", -1) as i16).name()
    };

    // 3. Linetype
    let lt_str = if has_sel {
        let lts: Vec<_> = sel.iter().filter_map(|h| d.entity(*h)).map(|e| e.common.linetype.clone()).collect();
        if lts.is_empty() {
            "ByLayer".into()
        } else if lts.iter().all(|l| l == &lts[0]) {
            lts[0].clone()
        } else {
            crate::tl!("*VARIES*").to_string()
        }
    } else {
        h.str("CELTYPE", "ByLayer")
    };

    let prop_w = 145.0;
    let mut change_color: Option<String> = None;
    let mut change_lw: Option<serde_json::Value> = None;
    let mut change_lt: Option<String> = None;

    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 2.0);
        ui.add_space(2.0);

        // Row 1: Color
        let (rect1, resp1) = ui.allocate_exact_size(vec2(prop_w, 20.0), Sense::click());
        if resp1.hovered() {
            ui.painter().rect_filled(rect1, 3.0, t.control_hover.gamma_multiply(0.6));
        } else {
            ui.painter().rect_filled(rect1, 3.0, t.control);
        }
        let cur_color = if color_str.eq_ignore_ascii_case("ByLayer") {
            Color::ByLayer
        } else if color_str.eq_ignore_ascii_case("ByBlock") {
            Color::ByBlock
        } else if let Ok(n) = color_str.parse::<i16>() {
            Color::from_aci(n)
        } else {
            Color::ByLayer
        };
        let swatch_rect = Rect::from_center_size(pos2(rect1.left() + 11.0, rect1.center().y), vec2(11.0, 11.0));
        crate::palettes::swatch(ui.painter(), swatch_rect, cur_color);
        let translated_color = crate::i18n::t(&color_str);
        let galley1 = ui.painter().layout_no_wrap(translated_color.to_string(), egui::FontId::proportional(10.5), t.text);
        ui.painter().galley(pos2(rect1.left() + 22.0, rect1.center().y - galley1.size().y / 2.0), galley1, t.text);
        icons::paint(ui.painter(), Rect::from_center_size(pos2(rect1.right() - 8.0, rect1.center().y), vec2(7.0, 7.0)), Icon::ChevronDown, false);
        icons::describe_control(ui, &resp1, &format!("Color: {color_str}"), None, false);
        let resp1 = resp1.on_hover_text(format!("{}: {translated_color}", crate::tl!("Color")));

        egui::Popup::from_toggle_button_response(&resp1).width(prop_w).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for (name, aci) in
                [("ByLayer", 256), ("ByBlock", 0), ("Red", 1), ("Yellow", 2), ("Green", 3), ("Cyan", 4), ("Blue", 5), ("Magenta", 6), ("White", 7)]
            {
                if ui.button(crate::i18n::t(name)).clicked() {
                    change_color = Some(aci.to_string());
                    ui.close();
                }
            }
        });

        // Row 2: Lineweight
        let (rect2, resp2) = ui.allocate_exact_size(vec2(prop_w, 20.0), Sense::click());
        if resp2.hovered() {
            ui.painter().rect_filled(rect2, 3.0, t.control_hover.gamma_multiply(0.6));
        } else {
            ui.painter().rect_filled(rect2, 3.0, t.control);
        }
        icons::paint(ui.painter(), Rect::from_min_size(pos2(rect2.left() + 4.0, rect2.top() + 2.0), vec2(16.0, 16.0)), Icon::Lineweight, false);
        let translated_lw = crate::i18n::t(&lw_str);
        let galley2 = ui.painter().layout_no_wrap(translated_lw.to_string(), egui::FontId::proportional(10.5), t.text);
        ui.painter().galley(pos2(rect2.left() + 24.0, rect2.center().y - galley2.size().y / 2.0), galley2, t.text);
        icons::paint(ui.painter(), Rect::from_center_size(pos2(rect2.right() - 8.0, rect2.center().y), vec2(7.0, 7.0)), Icon::ChevronDown, false);
        icons::describe_control(ui, &resp2, &format!("Lineweight: {lw_str}"), None, false);
        let resp2 = resp2.on_hover_text(format!("{}: {translated_lw}", crate::tl!("Lineweight")));

        egui::Popup::from_toggle_button_response(&resp2).width(prop_w).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for n in ["ByLayer", "ByBlock", "Default"] {
                if ui.button(crate::i18n::t(n)).clicked() {
                    change_lw = Some(serde_json::Value::String(n.to_string()));
                    ui.close();
                }
            }
            for v in cadcraft_doc::Lineweight::STANDARD {
                let label = format!("{:.2} mm", f64::from(v) / 100.0);
                if ui.button(label).clicked() {
                    change_lw = Some(serde_json::json!(f64::from(v) / 100.0));
                    ui.close();
                }
            }
        });

        // Row 3: Linetype
        let (rect3, resp3) = ui.allocate_exact_size(vec2(prop_w, 20.0), Sense::click());
        if resp3.hovered() {
            ui.painter().rect_filled(rect3, 3.0, t.control_hover.gamma_multiply(0.6));
        } else {
            ui.painter().rect_filled(rect3, 3.0, t.control);
        }
        icons::paint(ui.painter(), Rect::from_min_size(pos2(rect3.left() + 4.0, rect3.top() + 2.0), vec2(16.0, 16.0)), Icon::Line, false);
        let translated_lt = crate::i18n::t(&lt_str);
        let galley3 = ui.painter().layout_no_wrap(translated_lt.to_string(), egui::FontId::proportional(10.5), t.text);
        ui.painter().galley(pos2(rect3.left() + 24.0, rect3.center().y - galley3.size().y / 2.0), galley3, t.text);
        icons::paint(ui.painter(), Rect::from_center_size(pos2(rect3.right() - 8.0, rect3.center().y), vec2(7.0, 7.0)), Icon::ChevronDown, false);
        icons::describe_control(ui, &resp3, &format!("Linetype: {lt_str}"), None, false);
        let resp3 = resp3.on_hover_text(format!("{}: {translated_lt}", crate::tl!("Linetype")));

        egui::Popup::from_toggle_button_response(&resp3).width(prop_w).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            for n in ["ByLayer", "ByBlock"] {
                if ui.button(crate::i18n::t(n)).clicked() {
                    change_lt = Some(n.to_string());
                    ui.close();
                }
            }
            for lt in &d.linetypes {
                if !["ByLayer", "ByBlock"].contains(&lt.name.as_str()) && ui.button(&lt.name).clicked() {
                    change_lt = Some(lt.name.clone());
                    ui.close();
                }
            }
        });
    });

    if let Some(val) = change_color {
        if has_sel {
            let _ = app.run("properties.set", serde_json::json!({ "handles": ids, "color": val }));
        } else {
            let _ = app.run("color", serde_json::json!({ "color": val }));
        }
    }
    if let Some(val) = change_lw {
        if has_sel {
            let _ = app.run("properties.set", serde_json::json!({ "handles": ids, "lineweight": val }));
        } else {
            let _ = app.run("lweight", serde_json::json!({ "lineweight": val }));
        }
    }
    if let Some(val) = change_lt {
        if has_sel {
            let _ = app.run("properties.set", serde_json::json!({ "handles": ids, "linetype": val }));
        } else {
            let _ = app.run("linetype", serde_json::json!({ "current": val }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_tabs_have_panels_and_non_empty_titles() {
        assert!(!TABS.is_empty());
        for tab in TABS {
            assert!(!tab.id.is_empty());
            assert!(!tab.label.is_empty());
            assert!(!tab.panels.is_empty(), "tab {} has panels", tab.id);
            for panel in tab.panels {
                assert!(!panel.title.is_empty());
                assert!(!panel.items.is_empty(), "panel {} has items", panel.title);
            }
        }
    }

    #[test]
    fn home_tab_covers_core_drafting_panels() {
        let home = TABS.iter().find(|t| t.id == "Home");
        assert!(home.is_some(), "Home tab exists");
        let Some(home) = home else { return };
        let titles: Vec<_> = home.panels.iter().map(|p| p.title).collect();
        assert!(titles.contains(&"Draw"));
        assert!(titles.contains(&"Modify"));
        assert!(titles.contains(&"Annotation"));
        assert!(titles.contains(&"Layers"));
        assert!(titles.contains(&"Block"));
        assert!(titles.contains(&"Properties"));
        assert!(titles.contains(&"Utilities"));
    }

    #[test]
    fn commands_are_resolvable_or_valid_ui_commands() {
        let is_known = |id: &str| {
            cadcraft_engine::find_command(id).is_some()
                || cadcraft_engine::find_command(&cadcraft_engine::cmd::resolve_alias(id)).is_some()
                || crate::menus::is_ui_command(id)
        };

        for tab in TABS {
            for panel in tab.panels {
                for item in panel.items {
                    match item {
                        RibbonItem::Large { id, .. } => {
                            assert!(is_known(id), "command '{id}' in tab '{}' is known", tab.id);
                        }
                        RibbonItem::SplitButton { id, menu, .. } => {
                            assert!(is_known(id), "split command '{id}' in tab '{}' is known", tab.id);
                            for (m_cmd, _, _) in *menu {
                                assert!(is_known(m_cmd), "split menu command '{m_cmd}' in tab '{}' is known", tab.id);
                            }
                        }
                        RibbonItem::LargeDropdown { id, menu, .. } => {
                            assert!(is_known(id), "command '{id}' in tab '{}' is known", tab.id);
                            for (m_cmd, _, _) in *menu {
                                assert!(is_known(m_cmd), "menu command '{m_cmd}' in tab '{}' is known", tab.id);
                            }
                        }
                        RibbonItem::MediumGrid { cols } => {
                            for col in *cols {
                                for mi in *col {
                                    match mi {
                                        MediumItem::Button { id, .. } => {
                                            assert!(is_known(id), "medium command '{id}' in tab '{}' is known", tab.id);
                                        }
                                        MediumItem::Dropdown { id, menu, .. } => {
                                            assert!(is_known(id), "medium dropdown command '{id}' in tab '{}' is known", tab.id);
                                            for (m_cmd, _, _) in *menu {
                                                assert!(is_known(m_cmd), "medium menu command '{m_cmd}' in tab '{}' is known", tab.id);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        RibbonItem::CompactColumn(compact_items) => {
                            for ci in *compact_items {
                                match ci {
                                    CompactItem::Button { id, .. } | CompactItem::IconOnly { id, .. } => {
                                        assert!(is_known(id), "compact command '{id}' in tab '{}' is known", tab.id);
                                    }
                                    CompactItem::Dropdown { id, menu, .. } => {
                                        assert!(is_known(id), "compact command '{id}' in tab '{}' is known", tab.id);
                                        for (m_cmd, _, _) in *menu {
                                            assert!(is_known(m_cmd), "compact menu command '{m_cmd}' in tab '{}' is known", tab.id);
                                        }
                                    }
                                }
                            }
                        }
                        RibbonItem::IconColumn(icon_items) => {
                            for ii in *icon_items {
                                assert!(is_known(ii.id), "icon command '{}' in tab '{}' is known", ii.id, tab.id);
                            }
                        }
                        RibbonItem::LayersControl | RibbonItem::PropertiesControl => {}
                    }
                }
                for (cmd, _, _) in panel.more {
                    assert!(is_known(cmd), "more command '{cmd}' in panel '{}' is known", panel.title);
                }
            }
        }
    }
}
