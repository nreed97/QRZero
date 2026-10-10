import { useEffect, useState } from "react";
import { api } from "../api";
import type { UpdateInfo } from "../types";
import Modal from "./Modal";

const REPO = "https://github.com/nreed97/QRZero";

/** Name, installed version and where to find the project. Shows a newer release if the update check knows of one. */
export default function AboutDialog({ onClose }: { onClose: () => void }) {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  useEffect(() => {
    api.updates().then(setInfo).catch(() => {});
  }, []);
  const link = (href: string, text: string) => (
    <a href={href} target="_blank" rel="noopener noreferrer">{text}</a>
  );
  return (
    <Modal title="About QRZero" onClose={onClose} className="about">
      <p className="about-name"><b>QRZero</b></p>
      <p>Version {info ? info.current : "…"}</p>
      {info?.update && (
        <p>A newer version is available: {link(info.update.url, `${info.update.version}`)}</p>
      )}
      <p>
        A fast, reliable ham radio logbook. It keeps your log, works with your radios and the DX cluster,
        and sends and checks confirmations on LoTW, QRZ, Club Log and eQSL.
      </p>
      <ul className="about-links">
        <li>{link(REPO, "Project page on GitHub")}</li>
        <li>{link(`${REPO}/releases`, "Releases and downloads")}</li>
        <li>{link(`${REPO}/issues`, "Report a problem or suggest a change")}</li>
      </ul>
    </Modal>
  );
}
