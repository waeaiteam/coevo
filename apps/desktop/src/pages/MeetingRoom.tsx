import { useEffect, useState } from "react";
import { useParams, Link } from "react-router-dom";
import Icon from "../components/Icon";
import { SimpleMarkdown } from "../components/SimpleMarkdown";
import {
  listMeetings,
  getMeeting,
  startMeeting,
  type MeetingSummary,
  type MeetingDetail,
  type MeetingTurn,
} from "../api/org";
import { getActiveOpcId } from "../api/companies";
import { t, useLanguage } from "../settings/i18n";

function stanceClass(stance: string): "for" | "against" | "neutral" {
  if (stance === "for") return "for";
  if (stance === "against") return "against";
  return "neutral";
}

function stanceLabel(stance: string): string {
  if (stance === "for") return t("meet.stance_for");
  if (stance === "against") return t("meet.stance_against");
  return t("meet.stance_neutral");
}

function isConcluded(status: string): boolean {
  return status === "completed";
}

function statusLabel(status: string): string {
  return isConcluded(status) ? t("meet.status_completed") : t("meet.status_running");
}

export default function MeetingRoom() {
  useLanguage();
  const params = useParams();
  const opcId = params.opcId ? decodeURIComponent(params.opcId) : getActiveOpcId();

  const [meetings, setMeetings] = useState<MeetingSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string>("");
  const [detail, setDetail] = useState<MeetingDetail | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [topic, setTopic] = useState("");
  const [starting, setStarting] = useState(false);

  async function reloadList(autoSelect: boolean): Promise<MeetingSummary[]> {
    const rows = await listMeetings(opcId).catch(() => [] as MeetingSummary[]);
    setMeetings(rows);
    if (autoSelect) {
      setSelectedId((current) => (current ? current : rows[0]?.meeting_id ?? ""));
    }
    return rows;
  }

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setSelectedId("");
    setDetail(null);
    void listMeetings(opcId)
      .catch(() => [] as MeetingSummary[])
      .then((rows) => {
        if (!alive) return;
        setMeetings(rows);
        setSelectedId(rows[0]?.meeting_id ?? "");
        setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [opcId]);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let alive = true;
    setDetailLoading(true);
    void getMeeting(opcId, selectedId)
      .catch(() => null)
      .then((next) => {
        if (!alive) return;
        setDetail(next);
        setDetailLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [opcId, selectedId]);

  async function onStart() {
    const trimmed = topic.trim();
    if (!trimmed || starting) return;
    setStarting(true);
    try {
      const res = await startMeeting(opcId, {
        topic: trimmed,
        participants: [],
        close_mode: "chair",
      });
      await reloadList(false);
      setTopic("");
      setSelectedId(res.meeting_id);
    } catch {
      /* keep the composer state so the founder can retry */
    } finally {
      setStarting(false);
    }
  }

  return (
    <div className="product-page">
      <header className="product-header">
        <div className="min-w-0">
          <div className="product-kicker">{t("meet.kicker")}</div>
          <h1 className="product-title">{t("meet.title")}</h1>
          <p className="product-subtitle">{t("meet.subtitle")}</p>
        </div>
        <div className="product-actions">
          <Link className="product-link-button" to={`/companies/${encodeURIComponent(opcId)}`}>
            <Icon name="chevron-right" style={{ transform: "rotate(180deg)" }} />
            {t("companies.title")}
          </Link>
        </div>
      </header>

      <div className="product-grid-2">
        <div style={{ display: "grid", gap: 16 }}>
          <section className="product-panel">
            <div className="product-panel-heading">
              <h2>{t("meet.new_topic")}</h2>
            </div>
            <textarea
              className="composer-textarea"
              placeholder={t("meet.topic_placeholder")}
              value={topic}
              onChange={(event) => setTopic(event.target.value)}
              style={{
                minHeight: 70,
                border: "1px solid var(--border-subtle)",
                borderRadius: 8,
                padding: 10,
              }}
            />
            <div style={{ display: "flex", marginTop: 12 }}>
              <button
                type="button"
                className="primary-button"
                disabled={starting || topic.trim().length === 0}
                onClick={() => void onStart()}
              >
                {starting ? (
                  <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                    <Icon name="spinner" />
                    {t("meet.starting")}
                  </span>
                ) : (
                  t("meet.start")
                )}
              </button>
            </div>
          </section>

          <section className="product-panel">
            <div className="product-panel-heading">
              <h2>{t("meet.list")}</h2>
              <span>{meetings.length}</span>
            </div>
            {loading ? (
              <div className="product-empty">{t("settings.loading")}</div>
            ) : meetings.length === 0 ? (
              <div className="product-empty">{t("meet.empty")}</div>
            ) : (
              <div className="product-list">
                {meetings.map((meeting) => {
                  const selected = meeting.meeting_id === selectedId;
                  return (
                    <button
                      key={meeting.meeting_id}
                      type="button"
                      className="product-list-row"
                      onClick={() => setSelectedId(meeting.meeting_id)}
                      style={selected ? { borderColor: "var(--accent)" } : undefined}
                    >
                      <span className="product-row-main">{meeting.topic}</span>
                      <span className={`product-pill ${isConcluded(meeting.status) ? "green" : "blue"}`}>
                        {statusLabel(meeting.status)}
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
          </section>
        </div>

        <section className="product-panel">
          {!selectedId ? (
            <div className="empty-state">
              <div className="empty-state-icon">
                <Icon name="users" />
              </div>
              <p>{t("meet.select")}</p>
            </div>
          ) : detailLoading && !detail ? (
            <div className="product-empty">{t("settings.loading")}</div>
          ) : !detail ? (
            <div className="empty-state">
              <div className="empty-state-icon">
                <Icon name="users" />
              </div>
              <p>{t("meet.select")}</p>
            </div>
          ) : (
            <div style={{ display: "grid", gap: 18 }}>
              <div style={{ display: "flex", alignItems: "center", gap: 10, flexWrap: "wrap" }}>
                <h2 className="product-section-title" style={{ margin: 0 }}>
                  {detail.topic}
                </h2>
                <span className={`product-pill ${isConcluded(detail.status) ? "green" : "blue"}`}>
                  {statusLabel(detail.status)}
                </span>
              </div>

              <div>
                <div className="product-panel-heading">
                  <h2>{t("meet.agenda")}</h2>
                </div>
                <p className="product-prose">{detail.agenda}</p>
              </div>

              <div>
                <div className="product-panel-heading">
                  <h2>{t("meet.debate")}</h2>
                </div>
                {detail.transcript.length === 0 ? (
                  <div className="product-empty">{t("meet.select")}</div>
                ) : (
                  <div>
                    {detail.transcript.map((turn: MeetingTurn, index) => (
                      <div className="debate-turn" key={`${turn.agent_id}-${index}`}>
                        <div className="debate-avatar">
                          <Icon name="users" />
                        </div>
                        <div className="debate-body">
                          <div className="debate-head">
                            <span className="debate-agent">{turn.agent_id}</span>
                            <span className={`stance-chip ${stanceClass(turn.stance)}`}>
                              {stanceLabel(turn.stance)}
                            </span>
                          </div>
                          <div className="debate-text">{turn.text}</div>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>

              <div>
                <div className="product-panel-heading">
                  <h2>{t("meet.resolution")}</h2>
                </div>
                {detail.resolution_md ? (
                  <SimpleMarkdown content={detail.resolution_md} />
                ) : (
                  <div className="product-empty">{t("meet.select")}</div>
                )}
              </div>

              <div>
                <span className="mono-chip">
                  {t("meet.responsibility")}: {detail.responsibility_anchor}
                </span>
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
