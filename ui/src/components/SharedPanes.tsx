import { memo, useCallback, useState } from "react";
import type { PopContext } from "../bus";
import { localGet, localSet } from "../prefs";
import { useIntegrations, useRotator } from "../live";
import type { Fields, Qso, QsoFilter, SlotPick } from "../types";
import type { PaneId } from "../workspace";
import AwardsPaneView from "./AwardsPane";
import ClusterPaneView from "./ClusterPane";
import FtxMonitorView, { type DecodePick } from "./FtxMonitor";
import LookupPanel, { SlotsPane } from "./LookupPanel";
import MapPanelView, { type MapView } from "./MapPanel";
import WorkedPane from "./WorkedPane";
import BandMapPane from "./BandMapPane";
import WatchPane from "./WatchPane";
import ContestsPaneView from "./ContestsPane";
import DxpedPaneView from "./DxpedPane";
import NeededPaneView from "./NeededPane";
import PropagationPane from "./PropagationPane";
import RotatorPane from "./RotatorPane";
import NotesPane from "./NotesPane";
import ClubsPaneView from "./ClubsPane";
import EntryHost from "./EntryHost";

// Typing in the call box re-renders the whole app. These panes only depend on props that rarely
// change, so they skip those renders.
const AwardsPane = memo(AwardsPaneView);
const ClusterPane = memo(ClusterPaneView);
const FtxMonitor = memo(FtxMonitorView);
const DxpedPane = memo(DxpedPaneView);
const NeededPane = memo(NeededPaneView);
const ContestsPane = memo(ContestsPaneView);
const ClubsPane = memo(ClubsPaneView);
// The map is drawn again only when a position or a setting changes, not on every render.
const MapPanel = memo(MapPanelView, (a, b) => {
  const same = (p: typeof a.home, q: typeof b.home) => p === q || (!!p && !!q && p.lat === q.lat && p.lon === q.lon);
  return same(a.home, b.home) && same(a.dx, b.dx) && a.homeLabel === b.homeLabel && a.dxLabel === b.dxLabel && a.units === b.units
    && a.view === b.view && a.shade === b.shade && a.rotator === b.rotator && a.onView === b.onView && a.onShade === b.onShade;
});

export interface PaneActions {
  onPick: (p: DecodePick) => void;
  onEdit: (q: Qso) => void;
  onCopy: (f: Fields) => void;
  onShowQsos: (f: QsoFilter) => void;
  /** Lists a DXCC slot's QSOs in Worked before (null goes back to the call's history). */
  onSlot: (s: SlotPick | null) => void;
  onSettings: (tab: string) => void;
}

/** A pane that can live in the main window or in its own window. */
export default function SharedPane({ id, ctx, act }: { id: PaneId; ctx: PopContext; act: PaneActions }) {
  const integrations = useIntegrations();
  const rotatorAz = useRotator();
  const [mapView, setMapView] = useState<MapView>(localGet("qrzero.map", { view: "flat" as MapView }).view);
  const [mapShade, setMapShade] = useState<boolean>(localGet("qrzero.map", { shade: true }).shade);
  const { onSettings } = act;
  const clusterSettings = useCallback(() => onSettings("cluster"), [onSettings]);
  const onMapView = useCallback(
    (v: MapView) => {
      setMapView(v);
      localSet("qrzero.map", { view: v, shade: mapShade });
    },
    [mapShade],
  );
  const onMapShade = useCallback(
    (on: boolean) => {
      setMapShade(on);
      localSet("qrzero.map", { view: mapView, shade: on });
    },
    [mapView],
  );

  switch (id) {
    case "entry":
      return <EntryHost ctx={ctx} />;
    case "lookup":
      return <LookupPanel logId={ctx.logId} result={ctx.lookup} entry={ctx.entry} refreshKey={ctx.refreshKey} slot={ctx.slot} onSlot={act.onSlot} />;
    case "slots":
      return <SlotsPane logId={ctx.logId} result={ctx.lookup} entry={ctx.entry} refreshKey={ctx.refreshKey} slot={ctx.slot} onSlot={act.onSlot} />;
    case "worked":
      return (
        <WorkedPane
          logId={ctx.logId}
          call={ctx.entry.fields.CALL ?? ""}
          lookup={ctx.lookup}
          entry={ctx.entry}
          refreshKey={ctx.refreshKey}
          selectedId={ctx.editingId}
          slot={ctx.slot}
          onSlotClear={() => act.onSlot(null)}
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
          onView={onMapView}
          shade={mapShade}
          onShade={onMapShade}
        />
      );
    case "ftx":
      return <FtxMonitor mycall={ctx.stationCall} onPick={act.onPick} />;
    case "cluster":
      return <ClusterPane onPick={act.onPick} onSettings={clusterSettings} />;
    case "awards":
      return <AwardsPane logId={ctx.logId} callsigns={ctx.callsigns} onShowQsos={act.onShowQsos} />;
    case "bandmap":
      return <BandMapPane ctx={ctx} act={act} />;
    case "watch":
      return <WatchPane ctx={ctx} act={act} />;
    case "dxped":
      return <DxpedPane act={act} />;
    case "needed":
      return <NeededPane onPick={act.onPick} onSettings={clusterSettings} />;
    case "contests":
      return <ContestsPane />;
    case "propagation":
      return <PropagationPane ctx={ctx} act={act} />;
    case "rotator":
      return <RotatorPane ctx={ctx} act={act} />;
    case "notes":
      return <NotesPane ctx={ctx} act={act} />;
    case "clubs":
      return <ClubsPane />;
    default:
      return null;
  }
}
