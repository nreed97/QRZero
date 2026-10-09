import { useState } from "react";
import type { PopContext } from "../bus";
import { localGet, localSet } from "../prefs";
import { useIntegrations, useRotator } from "../live";
import type { Fields, Qso, QsoFilter } from "../types";
import type { PaneId } from "../workspace";
import AwardsPane from "./AwardsPane";
import ClusterPane from "./ClusterPane";
import FtxMonitor, { type DecodePick } from "./FtxMonitor";
import LookupPanel from "./LookupPanel";
import MapPanel, { type MapView } from "./MapPanel";
import WorkedPane from "./WorkedPane";
import BandMapPane from "./BandMapPane";
import WatchPane from "./WatchPane";
import ContestsPane from "./ContestsPane";
import DxpedPane from "./DxpedPane";
import PropagationPane from "./PropagationPane";
import RotatorPane from "./RotatorPane";
import NotesPane from "./NotesPane";

export interface PaneActions {
  onPick: (p: DecodePick) => void;
  onEdit: (q: Qso) => void;
  onCopy: (f: Fields) => void;
  onShowQsos: (f: QsoFilter) => void;
  onSettings: (tab: string) => void;
}

/** A pane that can live in the main window or in its own window. */
export default function SharedPane({ id, ctx, act }: { id: PaneId; ctx: PopContext; act: PaneActions }) {
  const integrations = useIntegrations();
  const rotatorAz = useRotator();
  const [mapView, setMapView] = useState<MapView>(localGet("qrzero.map", { view: "flat" as MapView }).view);
  const [mapShade, setMapShade] = useState<boolean>(localGet("qrzero.map", { shade: true }).shade);

  switch (id) {
    case "lookup":
      return <LookupPanel logId={ctx.logId} result={ctx.lookup} entry={ctx.entry} refreshKey={ctx.refreshKey} />;
    case "worked":
      return (
        <WorkedPane
          logId={ctx.logId}
          call={ctx.entry.fields.CALL ?? ""}
          lookup={ctx.lookup}
          entry={ctx.entry}
          refreshKey={ctx.refreshKey}
          selectedId={ctx.editingId}
          onEdit={act.onEdit}
          onCopy={act.onCopy}
          onShowInLog={(call) => act.onShowQsos({ call })}
        />
      );
    case "map":
      return (
        <MapPanel
          home={ctx.home}
          homeLabel={ctx.stationCall}
          dx={ctx.dx}
          dxLabel={ctx.dxLabel}
          units={ctx.units}
          rotator={integrations?.rotator_enabled ? rotatorAz : undefined}
          view={mapView}
          onView={(v) => {
            setMapView(v);
            localSet("qrzero.map", { view: v, shade: mapShade });
          }}
          shade={mapShade}
          onShade={(on) => {
            setMapShade(on);
            localSet("qrzero.map", { view: mapView, shade: on });
          }}
        />
      );
    case "ftx":
      return <FtxMonitor mycall={ctx.stationCall} onPick={act.onPick} />;
    case "cluster":
      return <ClusterPane onPick={act.onPick} onSettings={() => act.onSettings("cluster")} />;
    case "awards":
      return <AwardsPane logId={ctx.logId} callsigns={ctx.callsigns} onShowQsos={act.onShowQsos} />;
    case "bandmap":
      return <BandMapPane ctx={ctx} act={act} />;
    case "watch":
      return <WatchPane ctx={ctx} act={act} />;
    case "dxped":
      return <DxpedPane act={act} />;
    case "contests":
      return <ContestsPane />;
    case "propagation":
      return <PropagationPane ctx={ctx} act={act} />;
    case "rotator":
      return <RotatorPane ctx={ctx} act={act} />;
    case "notes":
      return <NotesPane ctx={ctx} act={act} />;
    default:
      return null;
  }
}
