use serde_repr::Deserialize_repr;

#[derive(Deserialize_repr)]
#[repr(u16)]
pub(super) enum ClientIdentifier {
    /// Ares
    AG,
    /// Ares
    #[serde(rename = "A~")]
    ATilde,
    /// Arctic
    AR,
    /// Avicora
    AV,
    /// BitPump
    AX,
    /// Azureus
    AZ,
    /// BitBuddy
    BB,
    /// BitComet
    BC,
    /// Bitflu
    BF,
    /// BTG (uses Rasterbar libtorrent)
    BG,
    /// BitRocket
    BR,
    /// BTSlave
    BS,
    /// ~Bittorrent X
    BX,
    /// Enhanced CTorrent
    CD,
    /// CTorrent
    CT,
    /// DelugeTorrent
    DE,
    /// Propagate Data Client
    DP,
    /// EBit
    EB,
    /// electric sheep
    ES,
    /// FoxTorrent
    FT,
    /// FrostWire
    FW,
    /// Freebox BitTorrent
    FX,
    /// GSTorrent
    GS,
    /// Halite
    HL,
    /// Hydranode
    HN,
    /// KGet
    KG,
    /// KTorrent
    KT,
    /// LABC
    LH,
    /// Lphant
    LP,
    /// libtorrent
    LT,
    /// libTorrent
    #[serde(rename = "lt")]
    Lt,
    /// LimeWire
    LW,
    /// MonoTorrent
    MO,
    /// MooPolice
    MP,
    /// Miro
    MR,
    /// MoonlightTorrent
    MT,
    /// Net Transport
    NX,
    /// Pando
    PD,
    /// qBittorrent
    #[serde(rename = "qB")]
    QB,
    /// QQDownload
    QD,
    /// Qt 4 Torrent example
    QT,
    /// Retriever
    RT,
    /// Shareaza alpha/beta
    #[serde(rename = "S~")]
    STilde,
    /// ~Swiftbit
    SB,
    /// SwarmScope
    SS,
    /// SymTorrent
    ST,
    /// sharktorrent
    #[serde(rename = "st")]
    St,
    /// Shareaza
    SZ,
    /// TorrentDotNET
    TN,
    /// Transmission
    TR,
    /// Torrentstorm
    TS,
    /// TuoTu
    TT,
    /// uLeecher!
    UL,
    /// µTorrent
    UT,
    /// µTorrent Web
    UW,
    /// Vagaa
    VG,
    /// WebTorrent Desktop
    WD,
    /// BitLet
    WT,
    /// WebTorrent
    WW,
    /// FireTorrent
    WY,
    /// Xunlei
    XL,
    /// XanTorrent
    XT,
    /// Xtorrent
    XX,
    /// ZipTorrent
    ZT,
}
