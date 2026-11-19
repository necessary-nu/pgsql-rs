// Generated from PostgreSQL 19.0 by tools/pg19/import-lexer.py.
pub(super) const COLUMNS: usize = 38;
impl Kind {
    pub const YYERROR: Self = Self(256);
    pub const YYUNDEF: Self = Self(257);
    pub const IDENT: Self = Self(258);
    pub const UIDENT: Self = Self(259);
    pub const FCONST: Self = Self(260);
    pub const SCONST: Self = Self(261);
    pub const USCONST: Self = Self(262);
    pub const BCONST: Self = Self(263);
    pub const XCONST: Self = Self(264);
    pub const OP: Self = Self(265);
    pub const ICONST: Self = Self(266);
    pub const PARAM: Self = Self(267);
    pub const TYPECAST: Self = Self(268);
    pub const DOT_DOT: Self = Self(269);
    pub const COLON_EQUALS: Self = Self(270);
    pub const EQUALS_GREATER: Self = Self(271);
    pub const LESS_EQUALS: Self = Self(272);
    pub const GREATER_EQUALS: Self = Self(273);
    pub const NOT_EQUALS: Self = Self(274);
    pub const SQL_COMMENT: Self = Self(275);
    pub const C_COMMENT: Self = Self(276);
    pub const ABORT_P: Self = Self(277);
    pub const ABSENT: Self = Self(278);
    pub const ABSOLUTE_P: Self = Self(279);
    pub const ACCESS: Self = Self(280);
    pub const ACTION: Self = Self(281);
    pub const ADD_P: Self = Self(282);
    pub const ADMIN: Self = Self(283);
    pub const AFTER: Self = Self(284);
    pub const AGGREGATE: Self = Self(285);
    pub const ALL: Self = Self(286);
    pub const ALSO: Self = Self(287);
    pub const ALTER: Self = Self(288);
    pub const ALWAYS: Self = Self(289);
    pub const ANALYSE: Self = Self(290);
    pub const ANALYZE: Self = Self(291);
    pub const AND: Self = Self(292);
    pub const ANY: Self = Self(293);
    pub const ARRAY: Self = Self(294);
    pub const AS: Self = Self(295);
    pub const ASC: Self = Self(296);
    pub const ASENSITIVE: Self = Self(297);
    pub const ASSERTION: Self = Self(298);
    pub const ASSIGNMENT: Self = Self(299);
    pub const ASYMMETRIC: Self = Self(300);
    pub const ATOMIC: Self = Self(301);
    pub const AT: Self = Self(302);
    pub const ATTACH: Self = Self(303);
    pub const ATTRIBUTE: Self = Self(304);
    pub const AUTHORIZATION: Self = Self(305);
    pub const BACKWARD: Self = Self(306);
    pub const BEFORE: Self = Self(307);
    pub const BEGIN_P: Self = Self(308);
    pub const BETWEEN: Self = Self(309);
    pub const BIGINT: Self = Self(310);
    pub const BINARY: Self = Self(311);
    pub const BIT: Self = Self(312);
    pub const BOOLEAN_P: Self = Self(313);
    pub const BOTH: Self = Self(314);
    pub const BREADTH: Self = Self(315);
    pub const BY: Self = Self(316);
    pub const CACHE: Self = Self(317);
    pub const CALL: Self = Self(318);
    pub const CALLED: Self = Self(319);
    pub const CASCADE: Self = Self(320);
    pub const CASCADED: Self = Self(321);
    pub const CASE: Self = Self(322);
    pub const CAST: Self = Self(323);
    pub const CATALOG_P: Self = Self(324);
    pub const CHAIN: Self = Self(325);
    pub const CHAR_P: Self = Self(326);
    pub const CHARACTER: Self = Self(327);
    pub const CHARACTERISTICS: Self = Self(328);
    pub const CHECK: Self = Self(329);
    pub const CHECKPOINT: Self = Self(330);
    pub const CLASS: Self = Self(331);
    pub const CLOSE: Self = Self(332);
    pub const CLUSTER: Self = Self(333);
    pub const COALESCE: Self = Self(334);
    pub const COLLATE: Self = Self(335);
    pub const COLLATION: Self = Self(336);
    pub const COLUMN: Self = Self(337);
    pub const COLUMNS: Self = Self(338);
    pub const COMMENT: Self = Self(339);
    pub const COMMENTS: Self = Self(340);
    pub const COMMIT: Self = Self(341);
    pub const COMMITTED: Self = Self(342);
    pub const COMPRESSION: Self = Self(343);
    pub const CONCURRENTLY: Self = Self(344);
    pub const CONDITIONAL: Self = Self(345);
    pub const CONFIGURATION: Self = Self(346);
    pub const CONFLICT: Self = Self(347);
    pub const CONNECTION: Self = Self(348);
    pub const CONSTRAINT: Self = Self(349);
    pub const CONSTRAINTS: Self = Self(350);
    pub const CONTENT_P: Self = Self(351);
    pub const CONTINUE_P: Self = Self(352);
    pub const CONVERSION_P: Self = Self(353);
    pub const COPY: Self = Self(354);
    pub const COST: Self = Self(355);
    pub const CREATE: Self = Self(356);
    pub const CROSS: Self = Self(357);
    pub const CSV: Self = Self(358);
    pub const CUBE: Self = Self(359);
    pub const CURRENT_P: Self = Self(360);
    pub const CURRENT_CATALOG: Self = Self(361);
    pub const CURRENT_DATE: Self = Self(362);
    pub const CURRENT_ROLE: Self = Self(363);
    pub const CURRENT_SCHEMA: Self = Self(364);
    pub const CURRENT_TIME: Self = Self(365);
    pub const CURRENT_TIMESTAMP: Self = Self(366);
    pub const CURRENT_USER: Self = Self(367);
    pub const CURSOR: Self = Self(368);
    pub const CYCLE: Self = Self(369);
    pub const DATA_P: Self = Self(370);
    pub const DATABASE: Self = Self(371);
    pub const DAY_P: Self = Self(372);
    pub const DEALLOCATE: Self = Self(373);
    pub const DEC: Self = Self(374);
    pub const DECIMAL_P: Self = Self(375);
    pub const DECLARE: Self = Self(376);
    pub const DEFAULT: Self = Self(377);
    pub const DEFAULTS: Self = Self(378);
    pub const DEFERRABLE: Self = Self(379);
    pub const DEFERRED: Self = Self(380);
    pub const DEFINER: Self = Self(381);
    pub const DELETE_P: Self = Self(382);
    pub const DELIMITER: Self = Self(383);
    pub const DELIMITERS: Self = Self(384);
    pub const DEPENDS: Self = Self(385);
    pub const DEPTH: Self = Self(386);
    pub const DESC: Self = Self(387);
    pub const DETACH: Self = Self(388);
    pub const DICTIONARY: Self = Self(389);
    pub const DISABLE_P: Self = Self(390);
    pub const DISCARD: Self = Self(391);
    pub const DISTINCT: Self = Self(392);
    pub const DO: Self = Self(393);
    pub const DOCUMENT_P: Self = Self(394);
    pub const DOMAIN_P: Self = Self(395);
    pub const DOUBLE_P: Self = Self(396);
    pub const DROP: Self = Self(397);
    pub const EACH: Self = Self(398);
    pub const ELSE: Self = Self(399);
    pub const EMPTY_P: Self = Self(400);
    pub const ENABLE_P: Self = Self(401);
    pub const ENCODING: Self = Self(402);
    pub const ENCRYPTED: Self = Self(403);
    pub const END_P: Self = Self(404);
    pub const ENFORCED: Self = Self(405);
    pub const ENUM_P: Self = Self(406);
    pub const ERROR_P: Self = Self(407);
    pub const ESCAPE: Self = Self(408);
    pub const EVENT: Self = Self(409);
    pub const EXCEPT: Self = Self(410);
    pub const EXCLUDE: Self = Self(411);
    pub const EXCLUDING: Self = Self(412);
    pub const EXCLUSIVE: Self = Self(413);
    pub const EXECUTE: Self = Self(414);
    pub const EXISTS: Self = Self(415);
    pub const EXPLAIN: Self = Self(416);
    pub const EXPRESSION: Self = Self(417);
    pub const EXTENSION: Self = Self(418);
    pub const EXTERNAL: Self = Self(419);
    pub const EXTRACT: Self = Self(420);
    pub const FALSE_P: Self = Self(421);
    pub const FAMILY: Self = Self(422);
    pub const FETCH: Self = Self(423);
    pub const FILTER: Self = Self(424);
    pub const FINALIZE: Self = Self(425);
    pub const FIRST_P: Self = Self(426);
    pub const FLOAT_P: Self = Self(427);
    pub const FOLLOWING: Self = Self(428);
    pub const FOR: Self = Self(429);
    pub const FORCE: Self = Self(430);
    pub const FOREIGN: Self = Self(431);
    pub const FORMAT: Self = Self(432);
    pub const FORWARD: Self = Self(433);
    pub const FREEZE: Self = Self(434);
    pub const FROM: Self = Self(435);
    pub const FULL: Self = Self(436);
    pub const FUNCTION: Self = Self(437);
    pub const FUNCTIONS: Self = Self(438);
    pub const GENERATED: Self = Self(439);
    pub const GLOBAL: Self = Self(440);
    pub const GRANT: Self = Self(441);
    pub const GRANTED: Self = Self(442);
    pub const GREATEST: Self = Self(443);
    pub const GROUP_P: Self = Self(444);
    pub const GROUPING: Self = Self(445);
    pub const GROUPS: Self = Self(446);
    pub const HANDLER: Self = Self(447);
    pub const HAVING: Self = Self(448);
    pub const HEADER_P: Self = Self(449);
    pub const HOLD: Self = Self(450);
    pub const HOUR_P: Self = Self(451);
    pub const IDENTITY_P: Self = Self(452);
    pub const IF_P: Self = Self(453);
    pub const IGNORE_P: Self = Self(454);
    pub const ILIKE: Self = Self(455);
    pub const IMMEDIATE: Self = Self(456);
    pub const IMMUTABLE: Self = Self(457);
    pub const IMPLICIT_P: Self = Self(458);
    pub const IMPORT_P: Self = Self(459);
    pub const IN_P: Self = Self(460);
    pub const INCLUDE: Self = Self(461);
    pub const INCLUDING: Self = Self(462);
    pub const INCREMENT: Self = Self(463);
    pub const INDENT: Self = Self(464);
    pub const INDEX: Self = Self(465);
    pub const INDEXES: Self = Self(466);
    pub const INHERIT: Self = Self(467);
    pub const INHERITS: Self = Self(468);
    pub const INITIALLY: Self = Self(469);
    pub const INLINE_P: Self = Self(470);
    pub const INNER_P: Self = Self(471);
    pub const INOUT: Self = Self(472);
    pub const INPUT_P: Self = Self(473);
    pub const INSENSITIVE: Self = Self(474);
    pub const INSERT: Self = Self(475);
    pub const INSTEAD: Self = Self(476);
    pub const INT_P: Self = Self(477);
    pub const INTEGER: Self = Self(478);
    pub const INTERSECT: Self = Self(479);
    pub const INTERVAL: Self = Self(480);
    pub const INTO: Self = Self(481);
    pub const INVOKER: Self = Self(482);
    pub const IS: Self = Self(483);
    pub const ISNULL: Self = Self(484);
    pub const ISOLATION: Self = Self(485);
    pub const JOIN: Self = Self(486);
    pub const JSON: Self = Self(487);
    pub const JSON_ARRAY: Self = Self(488);
    pub const JSON_ARRAYAGG: Self = Self(489);
    pub const JSON_EXISTS: Self = Self(490);
    pub const JSON_OBJECT: Self = Self(491);
    pub const JSON_OBJECTAGG: Self = Self(492);
    pub const JSON_QUERY: Self = Self(493);
    pub const JSON_SCALAR: Self = Self(494);
    pub const JSON_SERIALIZE: Self = Self(495);
    pub const JSON_TABLE: Self = Self(496);
    pub const JSON_VALUE: Self = Self(497);
    pub const KEEP: Self = Self(498);
    pub const KEY: Self = Self(499);
    pub const KEYS: Self = Self(500);
    pub const LABEL: Self = Self(501);
    pub const LANGUAGE: Self = Self(502);
    pub const LARGE_P: Self = Self(503);
    pub const LAST_P: Self = Self(504);
    pub const LATERAL_P: Self = Self(505);
    pub const LEADING: Self = Self(506);
    pub const LEAKPROOF: Self = Self(507);
    pub const LEAST: Self = Self(508);
    pub const LEFT: Self = Self(509);
    pub const LEVEL: Self = Self(510);
    pub const LIKE: Self = Self(511);
    pub const LIMIT: Self = Self(512);
    pub const LISTEN: Self = Self(513);
    pub const LOAD: Self = Self(514);
    pub const LOCAL: Self = Self(515);
    pub const LOCALTIME: Self = Self(516);
    pub const LOCALTIMESTAMP: Self = Self(517);
    pub const LOCATION: Self = Self(518);
    pub const LOCK_P: Self = Self(519);
    pub const LOCKED: Self = Self(520);
    pub const LOGGED: Self = Self(521);
    pub const LSN_P: Self = Self(522);
    pub const MAPPING: Self = Self(523);
    pub const MATCH: Self = Self(524);
    pub const MATCHED: Self = Self(525);
    pub const MATERIALIZED: Self = Self(526);
    pub const MAXVALUE: Self = Self(527);
    pub const MERGE: Self = Self(528);
    pub const MERGE_ACTION: Self = Self(529);
    pub const METHOD: Self = Self(530);
    pub const MINUTE_P: Self = Self(531);
    pub const MINVALUE: Self = Self(532);
    pub const MODE: Self = Self(533);
    pub const MONTH_P: Self = Self(534);
    pub const MOVE: Self = Self(535);
    pub const NAME_P: Self = Self(536);
    pub const NAMES: Self = Self(537);
    pub const NATIONAL: Self = Self(538);
    pub const NATURAL: Self = Self(539);
    pub const NCHAR: Self = Self(540);
    pub const NESTED: Self = Self(541);
    pub const NEW: Self = Self(542);
    pub const NEXT: Self = Self(543);
    pub const NFC: Self = Self(544);
    pub const NFD: Self = Self(545);
    pub const NFKC: Self = Self(546);
    pub const NFKD: Self = Self(547);
    pub const NO: Self = Self(548);
    pub const NONE: Self = Self(549);
    pub const NORMALIZE: Self = Self(550);
    pub const NORMALIZED: Self = Self(551);
    pub const NOT: Self = Self(552);
    pub const NOTHING: Self = Self(553);
    pub const NOTIFY: Self = Self(554);
    pub const NOTNULL: Self = Self(555);
    pub const NOWAIT: Self = Self(556);
    pub const NULL_P: Self = Self(557);
    pub const NULLIF: Self = Self(558);
    pub const NULLS_P: Self = Self(559);
    pub const NUMERIC: Self = Self(560);
    pub const OBJECT_P: Self = Self(561);
    pub const OBJECTS_P: Self = Self(562);
    pub const OF: Self = Self(563);
    pub const OFF: Self = Self(564);
    pub const OFFSET: Self = Self(565);
    pub const OIDS: Self = Self(566);
    pub const OLD: Self = Self(567);
    pub const OMIT: Self = Self(568);
    pub const ON: Self = Self(569);
    pub const ONLY: Self = Self(570);
    pub const OPERATOR: Self = Self(571);
    pub const OPTION: Self = Self(572);
    pub const OPTIONS: Self = Self(573);
    pub const OR: Self = Self(574);
    pub const ORDER: Self = Self(575);
    pub const ORDINALITY: Self = Self(576);
    pub const OTHERS: Self = Self(577);
    pub const OUT_P: Self = Self(578);
    pub const OUTER_P: Self = Self(579);
    pub const OVER: Self = Self(580);
    pub const OVERLAPS: Self = Self(581);
    pub const OVERLAY: Self = Self(582);
    pub const OVERRIDING: Self = Self(583);
    pub const OWNED: Self = Self(584);
    pub const OWNER: Self = Self(585);
    pub const PARALLEL: Self = Self(586);
    pub const PARAMETER: Self = Self(587);
    pub const PARSER: Self = Self(588);
    pub const PARTIAL: Self = Self(589);
    pub const PARTITION: Self = Self(590);
    pub const PASSING: Self = Self(591);
    pub const PASSWORD: Self = Self(592);
    pub const PATH: Self = Self(593);
    pub const PERIOD: Self = Self(594);
    pub const PLACING: Self = Self(595);
    pub const PLAN: Self = Self(596);
    pub const PLANS: Self = Self(597);
    pub const POLICY: Self = Self(598);
    pub const POSITION: Self = Self(599);
    pub const PRECEDING: Self = Self(600);
    pub const PRECISION: Self = Self(601);
    pub const PRESERVE: Self = Self(602);
    pub const PREPARE: Self = Self(603);
    pub const PREPARED: Self = Self(604);
    pub const PRIMARY: Self = Self(605);
    pub const PRIOR: Self = Self(606);
    pub const PRIVILEGES: Self = Self(607);
    pub const PROCEDURAL: Self = Self(608);
    pub const PROCEDURE: Self = Self(609);
    pub const PROCEDURES: Self = Self(610);
    pub const PROGRAM: Self = Self(611);
    pub const PUBLICATION: Self = Self(612);
    pub const QUOTE: Self = Self(613);
    pub const QUOTES: Self = Self(614);
    pub const RANGE: Self = Self(615);
    pub const READ: Self = Self(616);
    pub const REAL: Self = Self(617);
    pub const REASSIGN: Self = Self(618);
    pub const RECURSIVE: Self = Self(619);
    pub const REF_P: Self = Self(620);
    pub const REFERENCES: Self = Self(621);
    pub const REFERENCING: Self = Self(622);
    pub const REFRESH: Self = Self(623);
    pub const REINDEX: Self = Self(624);
    pub const RELATIVE_P: Self = Self(625);
    pub const RELEASE: Self = Self(626);
    pub const RENAME: Self = Self(627);
    pub const REPACK: Self = Self(628);
    pub const REPEATABLE: Self = Self(629);
    pub const REPLACE: Self = Self(630);
    pub const REPLICA: Self = Self(631);
    pub const RESET: Self = Self(632);
    pub const RESPECT_P: Self = Self(633);
    pub const RESTART: Self = Self(634);
    pub const RESTRICT: Self = Self(635);
    pub const RETURN: Self = Self(636);
    pub const RETURNING: Self = Self(637);
    pub const RETURNS: Self = Self(638);
    pub const REVOKE: Self = Self(639);
    pub const RIGHT: Self = Self(640);
    pub const ROLE: Self = Self(641);
    pub const ROLLBACK: Self = Self(642);
    pub const ROLLUP: Self = Self(643);
    pub const ROUTINE: Self = Self(644);
    pub const ROUTINES: Self = Self(645);
    pub const ROW: Self = Self(646);
    pub const ROWS: Self = Self(647);
    pub const RULE: Self = Self(648);
    pub const SAVEPOINT: Self = Self(649);
    pub const SCALAR: Self = Self(650);
    pub const SCHEMA: Self = Self(651);
    pub const SCHEMAS: Self = Self(652);
    pub const SCROLL: Self = Self(653);
    pub const SEARCH: Self = Self(654);
    pub const SECOND_P: Self = Self(655);
    pub const SECURITY: Self = Self(656);
    pub const SELECT: Self = Self(657);
    pub const SEQUENCE: Self = Self(658);
    pub const SEQUENCES: Self = Self(659);
    pub const SERIALIZABLE: Self = Self(660);
    pub const SERVER: Self = Self(661);
    pub const SESSION: Self = Self(662);
    pub const SESSION_USER: Self = Self(663);
    pub const SET: Self = Self(664);
    pub const SETS: Self = Self(665);
    pub const SETOF: Self = Self(666);
    pub const SHARE: Self = Self(667);
    pub const SHOW: Self = Self(668);
    pub const SIMILAR: Self = Self(669);
    pub const SIMPLE: Self = Self(670);
    pub const SKIP: Self = Self(671);
    pub const SMALLINT: Self = Self(672);
    pub const SNAPSHOT: Self = Self(673);
    pub const SOME: Self = Self(674);
    pub const SOURCE: Self = Self(675);
    pub const SQL_P: Self = Self(676);
    pub const STABLE: Self = Self(677);
    pub const STANDALONE_P: Self = Self(678);
    pub const START: Self = Self(679);
    pub const STATEMENT: Self = Self(680);
    pub const STATISTICS: Self = Self(681);
    pub const STDIN: Self = Self(682);
    pub const STDOUT: Self = Self(683);
    pub const STORAGE: Self = Self(684);
    pub const STORED: Self = Self(685);
    pub const STRICT_P: Self = Self(686);
    pub const STRING_P: Self = Self(687);
    pub const STRIP_P: Self = Self(688);
    pub const SUBSCRIPTION: Self = Self(689);
    pub const SUBSTRING: Self = Self(690);
    pub const SUPPORT: Self = Self(691);
    pub const SYMMETRIC: Self = Self(692);
    pub const SYSID: Self = Self(693);
    pub const SYSTEM_P: Self = Self(694);
    pub const SYSTEM_USER: Self = Self(695);
    pub const TABLE: Self = Self(696);
    pub const TABLES: Self = Self(697);
    pub const TABLESAMPLE: Self = Self(698);
    pub const TABLESPACE: Self = Self(699);
    pub const TARGET: Self = Self(700);
    pub const TEMP: Self = Self(701);
    pub const TEMPLATE: Self = Self(702);
    pub const TEMPORARY: Self = Self(703);
    pub const TEXT_P: Self = Self(704);
    pub const THEN: Self = Self(705);
    pub const TIES: Self = Self(706);
    pub const TIME: Self = Self(707);
    pub const TIMESTAMP: Self = Self(708);
    pub const TO: Self = Self(709);
    pub const TRAILING: Self = Self(710);
    pub const TRANSACTION: Self = Self(711);
    pub const TRANSFORM: Self = Self(712);
    pub const TREAT: Self = Self(713);
    pub const TRIGGER: Self = Self(714);
    pub const TRIM: Self = Self(715);
    pub const TRUE_P: Self = Self(716);
    pub const TRUNCATE: Self = Self(717);
    pub const TRUSTED: Self = Self(718);
    pub const TYPE_P: Self = Self(719);
    pub const TYPES_P: Self = Self(720);
    pub const UESCAPE: Self = Self(721);
    pub const UNBOUNDED: Self = Self(722);
    pub const UNCONDITIONAL: Self = Self(723);
    pub const UNCOMMITTED: Self = Self(724);
    pub const UNENCRYPTED: Self = Self(725);
    pub const UNION: Self = Self(726);
    pub const UNIQUE: Self = Self(727);
    pub const UNKNOWN: Self = Self(728);
    pub const UNLISTEN: Self = Self(729);
    pub const UNLOGGED: Self = Self(730);
    pub const UNTIL: Self = Self(731);
    pub const UPDATE: Self = Self(732);
    pub const USER: Self = Self(733);
    pub const USING: Self = Self(734);
    pub const VACUUM: Self = Self(735);
    pub const VALID: Self = Self(736);
    pub const VALIDATE: Self = Self(737);
    pub const VALIDATOR: Self = Self(738);
    pub const VALUE_P: Self = Self(739);
    pub const VALUES: Self = Self(740);
    pub const VARCHAR: Self = Self(741);
    pub const VARIADIC: Self = Self(742);
    pub const VARYING: Self = Self(743);
    pub const VERBOSE: Self = Self(744);
    pub const VERSION_P: Self = Self(745);
    pub const VIEW: Self = Self(746);
    pub const VIEWS: Self = Self(747);
    pub const VIRTUAL: Self = Self(748);
    pub const VOLATILE: Self = Self(749);
    pub const WAIT: Self = Self(750);
    pub const WHEN: Self = Self(751);
    pub const WHERE: Self = Self(752);
    pub const WHITESPACE_P: Self = Self(753);
    pub const WINDOW: Self = Self(754);
    pub const WITH: Self = Self(755);
    pub const WITHIN: Self = Self(756);
    pub const WITHOUT: Self = Self(757);
    pub const WORK: Self = Self(758);
    pub const WRAPPER: Self = Self(759);
    pub const WRITE: Self = Self(760);
    pub const XML_P: Self = Self(761);
    pub const XMLATTRIBUTES: Self = Self(762);
    pub const XMLCONCAT: Self = Self(763);
    pub const XMLELEMENT: Self = Self(764);
    pub const XMLEXISTS: Self = Self(765);
    pub const XMLFOREST: Self = Self(766);
    pub const XMLNAMESPACES: Self = Self(767);
    pub const XMLPARSE: Self = Self(768);
    pub const XMLPI: Self = Self(769);
    pub const XMLROOT: Self = Self(770);
    pub const XMLSERIALIZE: Self = Self(771);
    pub const XMLTABLE: Self = Self(772);
    pub const YEAR_P: Self = Self(773);
    pub const YES_P: Self = Self(774);
    pub const ZONE: Self = Self(775);
    pub const FORMAT_LA: Self = Self(776);
    pub const NOT_LA: Self = Self(777);
    pub const NULLS_LA: Self = Self(778);
    pub const WITH_LA: Self = Self(779);
    pub const WITHOUT_LA: Self = Self(780);
    pub const MODE_TYPE_NAME: Self = Self(781);
    pub const MODE_PLPGSQL_EXPR: Self = Self(782);
    pub const MODE_PLPGSQL_ASSIGN1: Self = Self(783);
    pub const MODE_PLPGSQL_ASSIGN2: Self = Self(784);
    pub const MODE_PLPGSQL_ASSIGN3: Self = Self(785);
    pub const UMINUS: Self = Self(786);
}
pub(super) static NAMES: &[&str] = &[
    "YYerror",
    "YYUNDEF",
    "IDENT",
    "UIDENT",
    "FCONST",
    "SCONST",
    "USCONST",
    "BCONST",
    "XCONST",
    "Op",
    "ICONST",
    "PARAM",
    "TYPECAST",
    "DOT_DOT",
    "COLON_EQUALS",
    "EQUALS_GREATER",
    "LESS_EQUALS",
    "GREATER_EQUALS",
    "NOT_EQUALS",
    "SQL_COMMENT",
    "C_COMMENT",
    "ABORT_P",
    "ABSENT",
    "ABSOLUTE_P",
    "ACCESS",
    "ACTION",
    "ADD_P",
    "ADMIN",
    "AFTER",
    "AGGREGATE",
    "ALL",
    "ALSO",
    "ALTER",
    "ALWAYS",
    "ANALYSE",
    "ANALYZE",
    "AND",
    "ANY",
    "ARRAY",
    "AS",
    "ASC",
    "ASENSITIVE",
    "ASSERTION",
    "ASSIGNMENT",
    "ASYMMETRIC",
    "ATOMIC",
    "AT",
    "ATTACH",
    "ATTRIBUTE",
    "AUTHORIZATION",
    "BACKWARD",
    "BEFORE",
    "BEGIN_P",
    "BETWEEN",
    "BIGINT",
    "BINARY",
    "BIT",
    "BOOLEAN_P",
    "BOTH",
    "BREADTH",
    "BY",
    "CACHE",
    "CALL",
    "CALLED",
    "CASCADE",
    "CASCADED",
    "CASE",
    "CAST",
    "CATALOG_P",
    "CHAIN",
    "CHAR_P",
    "CHARACTER",
    "CHARACTERISTICS",
    "CHECK",
    "CHECKPOINT",
    "CLASS",
    "CLOSE",
    "CLUSTER",
    "COALESCE",
    "COLLATE",
    "COLLATION",
    "COLUMN",
    "COLUMNS",
    "COMMENT",
    "COMMENTS",
    "COMMIT",
    "COMMITTED",
    "COMPRESSION",
    "CONCURRENTLY",
    "CONDITIONAL",
    "CONFIGURATION",
    "CONFLICT",
    "CONNECTION",
    "CONSTRAINT",
    "CONSTRAINTS",
    "CONTENT_P",
    "CONTINUE_P",
    "CONVERSION_P",
    "COPY",
    "COST",
    "CREATE",
    "CROSS",
    "CSV",
    "CUBE",
    "CURRENT_P",
    "CURRENT_CATALOG",
    "CURRENT_DATE",
    "CURRENT_ROLE",
    "CURRENT_SCHEMA",
    "CURRENT_TIME",
    "CURRENT_TIMESTAMP",
    "CURRENT_USER",
    "CURSOR",
    "CYCLE",
    "DATA_P",
    "DATABASE",
    "DAY_P",
    "DEALLOCATE",
    "DEC",
    "DECIMAL_P",
    "DECLARE",
    "DEFAULT",
    "DEFAULTS",
    "DEFERRABLE",
    "DEFERRED",
    "DEFINER",
    "DELETE_P",
    "DELIMITER",
    "DELIMITERS",
    "DEPENDS",
    "DEPTH",
    "DESC",
    "DETACH",
    "DICTIONARY",
    "DISABLE_P",
    "DISCARD",
    "DISTINCT",
    "DO",
    "DOCUMENT_P",
    "DOMAIN_P",
    "DOUBLE_P",
    "DROP",
    "EACH",
    "ELSE",
    "EMPTY_P",
    "ENABLE_P",
    "ENCODING",
    "ENCRYPTED",
    "END_P",
    "ENFORCED",
    "ENUM_P",
    "ERROR_P",
    "ESCAPE",
    "EVENT",
    "EXCEPT",
    "EXCLUDE",
    "EXCLUDING",
    "EXCLUSIVE",
    "EXECUTE",
    "EXISTS",
    "EXPLAIN",
    "EXPRESSION",
    "EXTENSION",
    "EXTERNAL",
    "EXTRACT",
    "FALSE_P",
    "FAMILY",
    "FETCH",
    "FILTER",
    "FINALIZE",
    "FIRST_P",
    "FLOAT_P",
    "FOLLOWING",
    "FOR",
    "FORCE",
    "FOREIGN",
    "FORMAT",
    "FORWARD",
    "FREEZE",
    "FROM",
    "FULL",
    "FUNCTION",
    "FUNCTIONS",
    "GENERATED",
    "GLOBAL",
    "GRANT",
    "GRANTED",
    "GREATEST",
    "GROUP_P",
    "GROUPING",
    "GROUPS",
    "HANDLER",
    "HAVING",
    "HEADER_P",
    "HOLD",
    "HOUR_P",
    "IDENTITY_P",
    "IF_P",
    "IGNORE_P",
    "ILIKE",
    "IMMEDIATE",
    "IMMUTABLE",
    "IMPLICIT_P",
    "IMPORT_P",
    "IN_P",
    "INCLUDE",
    "INCLUDING",
    "INCREMENT",
    "INDENT",
    "INDEX",
    "INDEXES",
    "INHERIT",
    "INHERITS",
    "INITIALLY",
    "INLINE_P",
    "INNER_P",
    "INOUT",
    "INPUT_P",
    "INSENSITIVE",
    "INSERT",
    "INSTEAD",
    "INT_P",
    "INTEGER",
    "INTERSECT",
    "INTERVAL",
    "INTO",
    "INVOKER",
    "IS",
    "ISNULL",
    "ISOLATION",
    "JOIN",
    "JSON",
    "JSON_ARRAY",
    "JSON_ARRAYAGG",
    "JSON_EXISTS",
    "JSON_OBJECT",
    "JSON_OBJECTAGG",
    "JSON_QUERY",
    "JSON_SCALAR",
    "JSON_SERIALIZE",
    "JSON_TABLE",
    "JSON_VALUE",
    "KEEP",
    "KEY",
    "KEYS",
    "LABEL",
    "LANGUAGE",
    "LARGE_P",
    "LAST_P",
    "LATERAL_P",
    "LEADING",
    "LEAKPROOF",
    "LEAST",
    "LEFT",
    "LEVEL",
    "LIKE",
    "LIMIT",
    "LISTEN",
    "LOAD",
    "LOCAL",
    "LOCALTIME",
    "LOCALTIMESTAMP",
    "LOCATION",
    "LOCK_P",
    "LOCKED",
    "LOGGED",
    "LSN_P",
    "MAPPING",
    "MATCH",
    "MATCHED",
    "MATERIALIZED",
    "MAXVALUE",
    "MERGE",
    "MERGE_ACTION",
    "METHOD",
    "MINUTE_P",
    "MINVALUE",
    "MODE",
    "MONTH_P",
    "MOVE",
    "NAME_P",
    "NAMES",
    "NATIONAL",
    "NATURAL",
    "NCHAR",
    "NESTED",
    "NEW",
    "NEXT",
    "NFC",
    "NFD",
    "NFKC",
    "NFKD",
    "NO",
    "NONE",
    "NORMALIZE",
    "NORMALIZED",
    "NOT",
    "NOTHING",
    "NOTIFY",
    "NOTNULL",
    "NOWAIT",
    "NULL_P",
    "NULLIF",
    "NULLS_P",
    "NUMERIC",
    "OBJECT_P",
    "OBJECTS_P",
    "OF",
    "OFF",
    "OFFSET",
    "OIDS",
    "OLD",
    "OMIT",
    "ON",
    "ONLY",
    "OPERATOR",
    "OPTION",
    "OPTIONS",
    "OR",
    "ORDER",
    "ORDINALITY",
    "OTHERS",
    "OUT_P",
    "OUTER_P",
    "OVER",
    "OVERLAPS",
    "OVERLAY",
    "OVERRIDING",
    "OWNED",
    "OWNER",
    "PARALLEL",
    "PARAMETER",
    "PARSER",
    "PARTIAL",
    "PARTITION",
    "PASSING",
    "PASSWORD",
    "PATH",
    "PERIOD",
    "PLACING",
    "PLAN",
    "PLANS",
    "POLICY",
    "POSITION",
    "PRECEDING",
    "PRECISION",
    "PRESERVE",
    "PREPARE",
    "PREPARED",
    "PRIMARY",
    "PRIOR",
    "PRIVILEGES",
    "PROCEDURAL",
    "PROCEDURE",
    "PROCEDURES",
    "PROGRAM",
    "PUBLICATION",
    "QUOTE",
    "QUOTES",
    "RANGE",
    "READ",
    "REAL",
    "REASSIGN",
    "RECURSIVE",
    "REF_P",
    "REFERENCES",
    "REFERENCING",
    "REFRESH",
    "REINDEX",
    "RELATIVE_P",
    "RELEASE",
    "RENAME",
    "REPACK",
    "REPEATABLE",
    "REPLACE",
    "REPLICA",
    "RESET",
    "RESPECT_P",
    "RESTART",
    "RESTRICT",
    "RETURN",
    "RETURNING",
    "RETURNS",
    "REVOKE",
    "RIGHT",
    "ROLE",
    "ROLLBACK",
    "ROLLUP",
    "ROUTINE",
    "ROUTINES",
    "ROW",
    "ROWS",
    "RULE",
    "SAVEPOINT",
    "SCALAR",
    "SCHEMA",
    "SCHEMAS",
    "SCROLL",
    "SEARCH",
    "SECOND_P",
    "SECURITY",
    "SELECT",
    "SEQUENCE",
    "SEQUENCES",
    "SERIALIZABLE",
    "SERVER",
    "SESSION",
    "SESSION_USER",
    "SET",
    "SETS",
    "SETOF",
    "SHARE",
    "SHOW",
    "SIMILAR",
    "SIMPLE",
    "SKIP",
    "SMALLINT",
    "SNAPSHOT",
    "SOME",
    "SOURCE",
    "SQL_P",
    "STABLE",
    "STANDALONE_P",
    "START",
    "STATEMENT",
    "STATISTICS",
    "STDIN",
    "STDOUT",
    "STORAGE",
    "STORED",
    "STRICT_P",
    "STRING_P",
    "STRIP_P",
    "SUBSCRIPTION",
    "SUBSTRING",
    "SUPPORT",
    "SYMMETRIC",
    "SYSID",
    "SYSTEM_P",
    "SYSTEM_USER",
    "TABLE",
    "TABLES",
    "TABLESAMPLE",
    "TABLESPACE",
    "TARGET",
    "TEMP",
    "TEMPLATE",
    "TEMPORARY",
    "TEXT_P",
    "THEN",
    "TIES",
    "TIME",
    "TIMESTAMP",
    "TO",
    "TRAILING",
    "TRANSACTION",
    "TRANSFORM",
    "TREAT",
    "TRIGGER",
    "TRIM",
    "TRUE_P",
    "TRUNCATE",
    "TRUSTED",
    "TYPE_P",
    "TYPES_P",
    "UESCAPE",
    "UNBOUNDED",
    "UNCONDITIONAL",
    "UNCOMMITTED",
    "UNENCRYPTED",
    "UNION",
    "UNIQUE",
    "UNKNOWN",
    "UNLISTEN",
    "UNLOGGED",
    "UNTIL",
    "UPDATE",
    "USER",
    "USING",
    "VACUUM",
    "VALID",
    "VALIDATE",
    "VALIDATOR",
    "VALUE_P",
    "VALUES",
    "VARCHAR",
    "VARIADIC",
    "VARYING",
    "VERBOSE",
    "VERSION_P",
    "VIEW",
    "VIEWS",
    "VIRTUAL",
    "VOLATILE",
    "WAIT",
    "WHEN",
    "WHERE",
    "WHITESPACE_P",
    "WINDOW",
    "WITH",
    "WITHIN",
    "WITHOUT",
    "WORK",
    "WRAPPER",
    "WRITE",
    "XML_P",
    "XMLATTRIBUTES",
    "XMLCONCAT",
    "XMLELEMENT",
    "XMLEXISTS",
    "XMLFOREST",
    "XMLNAMESPACES",
    "XMLPARSE",
    "XMLPI",
    "XMLROOT",
    "XMLSERIALIZE",
    "XMLTABLE",
    "YEAR_P",
    "YES_P",
    "ZONE",
    "FORMAT_LA",
    "NOT_LA",
    "NULLS_LA",
    "WITH_LA",
    "WITHOUT_LA",
    "MODE_TYPE_NAME",
    "MODE_PLPGSQL_EXPR",
    "MODE_PLPGSQL_ASSIGN1",
    "MODE_PLPGSQL_ASSIGN2",
    "MODE_PLPGSQL_ASSIGN3",
    "UMINUS",
];
pub(super) static KEYWORDS: &[(&str, Kind, KeywordKind)] = &[
    ("abort", Kind::ABORT_P, KeywordKind::Unreserved),
    ("absent", Kind::ABSENT, KeywordKind::Unreserved),
    ("absolute", Kind::ABSOLUTE_P, KeywordKind::Unreserved),
    ("access", Kind::ACCESS, KeywordKind::Unreserved),
    ("action", Kind::ACTION, KeywordKind::Unreserved),
    ("add", Kind::ADD_P, KeywordKind::Unreserved),
    ("admin", Kind::ADMIN, KeywordKind::Unreserved),
    ("after", Kind::AFTER, KeywordKind::Unreserved),
    ("aggregate", Kind::AGGREGATE, KeywordKind::Unreserved),
    ("all", Kind::ALL, KeywordKind::Reserved),
    ("also", Kind::ALSO, KeywordKind::Unreserved),
    ("alter", Kind::ALTER, KeywordKind::Unreserved),
    ("always", Kind::ALWAYS, KeywordKind::Unreserved),
    ("analyse", Kind::ANALYSE, KeywordKind::Reserved),
    ("analyze", Kind::ANALYZE, KeywordKind::Reserved),
    ("and", Kind::AND, KeywordKind::Reserved),
    ("any", Kind::ANY, KeywordKind::Reserved),
    ("array", Kind::ARRAY, KeywordKind::Reserved),
    ("as", Kind::AS, KeywordKind::Reserved),
    ("asc", Kind::ASC, KeywordKind::Reserved),
    ("asensitive", Kind::ASENSITIVE, KeywordKind::Unreserved),
    ("assertion", Kind::ASSERTION, KeywordKind::Unreserved),
    ("assignment", Kind::ASSIGNMENT, KeywordKind::Unreserved),
    ("asymmetric", Kind::ASYMMETRIC, KeywordKind::Reserved),
    ("at", Kind::AT, KeywordKind::Unreserved),
    ("atomic", Kind::ATOMIC, KeywordKind::Unreserved),
    ("attach", Kind::ATTACH, KeywordKind::Unreserved),
    ("attribute", Kind::ATTRIBUTE, KeywordKind::Unreserved),
    ("authorization", Kind::AUTHORIZATION, KeywordKind::TypeFunctionName),
    ("backward", Kind::BACKWARD, KeywordKind::Unreserved),
    ("before", Kind::BEFORE, KeywordKind::Unreserved),
    ("begin", Kind::BEGIN_P, KeywordKind::Unreserved),
    ("between", Kind::BETWEEN, KeywordKind::ColumnName),
    ("bigint", Kind::BIGINT, KeywordKind::ColumnName),
    ("binary", Kind::BINARY, KeywordKind::TypeFunctionName),
    ("bit", Kind::BIT, KeywordKind::ColumnName),
    ("boolean", Kind::BOOLEAN_P, KeywordKind::ColumnName),
    ("both", Kind::BOTH, KeywordKind::Reserved),
    ("breadth", Kind::BREADTH, KeywordKind::Unreserved),
    ("by", Kind::BY, KeywordKind::Unreserved),
    ("cache", Kind::CACHE, KeywordKind::Unreserved),
    ("call", Kind::CALL, KeywordKind::Unreserved),
    ("called", Kind::CALLED, KeywordKind::Unreserved),
    ("cascade", Kind::CASCADE, KeywordKind::Unreserved),
    ("cascaded", Kind::CASCADED, KeywordKind::Unreserved),
    ("case", Kind::CASE, KeywordKind::Reserved),
    ("cast", Kind::CAST, KeywordKind::Reserved),
    ("catalog", Kind::CATALOG_P, KeywordKind::Unreserved),
    ("chain", Kind::CHAIN, KeywordKind::Unreserved),
    ("char", Kind::CHAR_P, KeywordKind::ColumnName),
    ("character", Kind::CHARACTER, KeywordKind::ColumnName),
    ("characteristics", Kind::CHARACTERISTICS, KeywordKind::Unreserved),
    ("check", Kind::CHECK, KeywordKind::Reserved),
    ("checkpoint", Kind::CHECKPOINT, KeywordKind::Unreserved),
    ("class", Kind::CLASS, KeywordKind::Unreserved),
    ("close", Kind::CLOSE, KeywordKind::Unreserved),
    ("cluster", Kind::CLUSTER, KeywordKind::Unreserved),
    ("coalesce", Kind::COALESCE, KeywordKind::ColumnName),
    ("collate", Kind::COLLATE, KeywordKind::Reserved),
    ("collation", Kind::COLLATION, KeywordKind::TypeFunctionName),
    ("column", Kind::COLUMN, KeywordKind::Reserved),
    ("columns", Kind::COLUMNS, KeywordKind::Unreserved),
    ("comment", Kind::COMMENT, KeywordKind::Unreserved),
    ("comments", Kind::COMMENTS, KeywordKind::Unreserved),
    ("commit", Kind::COMMIT, KeywordKind::Unreserved),
    ("committed", Kind::COMMITTED, KeywordKind::Unreserved),
    ("compression", Kind::COMPRESSION, KeywordKind::Unreserved),
    ("concurrently", Kind::CONCURRENTLY, KeywordKind::TypeFunctionName),
    ("conditional", Kind::CONDITIONAL, KeywordKind::Unreserved),
    ("configuration", Kind::CONFIGURATION, KeywordKind::Unreserved),
    ("conflict", Kind::CONFLICT, KeywordKind::Unreserved),
    ("connection", Kind::CONNECTION, KeywordKind::Unreserved),
    ("constraint", Kind::CONSTRAINT, KeywordKind::Reserved),
    ("constraints", Kind::CONSTRAINTS, KeywordKind::Unreserved),
    ("content", Kind::CONTENT_P, KeywordKind::Unreserved),
    ("continue", Kind::CONTINUE_P, KeywordKind::Unreserved),
    ("conversion", Kind::CONVERSION_P, KeywordKind::Unreserved),
    ("copy", Kind::COPY, KeywordKind::Unreserved),
    ("cost", Kind::COST, KeywordKind::Unreserved),
    ("create", Kind::CREATE, KeywordKind::Reserved),
    ("cross", Kind::CROSS, KeywordKind::TypeFunctionName),
    ("csv", Kind::CSV, KeywordKind::Unreserved),
    ("cube", Kind::CUBE, KeywordKind::Unreserved),
    ("current", Kind::CURRENT_P, KeywordKind::Unreserved),
    ("current_catalog", Kind::CURRENT_CATALOG, KeywordKind::Reserved),
    ("current_date", Kind::CURRENT_DATE, KeywordKind::Reserved),
    ("current_role", Kind::CURRENT_ROLE, KeywordKind::Reserved),
    ("current_schema", Kind::CURRENT_SCHEMA, KeywordKind::TypeFunctionName),
    ("current_time", Kind::CURRENT_TIME, KeywordKind::Reserved),
    ("current_timestamp", Kind::CURRENT_TIMESTAMP, KeywordKind::Reserved),
    ("current_user", Kind::CURRENT_USER, KeywordKind::Reserved),
    ("cursor", Kind::CURSOR, KeywordKind::Unreserved),
    ("cycle", Kind::CYCLE, KeywordKind::Unreserved),
    ("data", Kind::DATA_P, KeywordKind::Unreserved),
    ("database", Kind::DATABASE, KeywordKind::Unreserved),
    ("day", Kind::DAY_P, KeywordKind::Unreserved),
    ("deallocate", Kind::DEALLOCATE, KeywordKind::Unreserved),
    ("dec", Kind::DEC, KeywordKind::ColumnName),
    ("decimal", Kind::DECIMAL_P, KeywordKind::ColumnName),
    ("declare", Kind::DECLARE, KeywordKind::Unreserved),
    ("default", Kind::DEFAULT, KeywordKind::Reserved),
    ("defaults", Kind::DEFAULTS, KeywordKind::Unreserved),
    ("deferrable", Kind::DEFERRABLE, KeywordKind::Reserved),
    ("deferred", Kind::DEFERRED, KeywordKind::Unreserved),
    ("definer", Kind::DEFINER, KeywordKind::Unreserved),
    ("delete", Kind::DELETE_P, KeywordKind::Unreserved),
    ("delimiter", Kind::DELIMITER, KeywordKind::Unreserved),
    ("delimiters", Kind::DELIMITERS, KeywordKind::Unreserved),
    ("depends", Kind::DEPENDS, KeywordKind::Unreserved),
    ("depth", Kind::DEPTH, KeywordKind::Unreserved),
    ("desc", Kind::DESC, KeywordKind::Reserved),
    ("detach", Kind::DETACH, KeywordKind::Unreserved),
    ("dictionary", Kind::DICTIONARY, KeywordKind::Unreserved),
    ("disable", Kind::DISABLE_P, KeywordKind::Unreserved),
    ("discard", Kind::DISCARD, KeywordKind::Unreserved),
    ("distinct", Kind::DISTINCT, KeywordKind::Reserved),
    ("do", Kind::DO, KeywordKind::Reserved),
    ("document", Kind::DOCUMENT_P, KeywordKind::Unreserved),
    ("domain", Kind::DOMAIN_P, KeywordKind::Unreserved),
    ("double", Kind::DOUBLE_P, KeywordKind::Unreserved),
    ("drop", Kind::DROP, KeywordKind::Unreserved),
    ("each", Kind::EACH, KeywordKind::Unreserved),
    ("else", Kind::ELSE, KeywordKind::Reserved),
    ("empty", Kind::EMPTY_P, KeywordKind::Unreserved),
    ("enable", Kind::ENABLE_P, KeywordKind::Unreserved),
    ("encoding", Kind::ENCODING, KeywordKind::Unreserved),
    ("encrypted", Kind::ENCRYPTED, KeywordKind::Unreserved),
    ("end", Kind::END_P, KeywordKind::Reserved),
    ("enforced", Kind::ENFORCED, KeywordKind::Unreserved),
    ("enum", Kind::ENUM_P, KeywordKind::Unreserved),
    ("error", Kind::ERROR_P, KeywordKind::Unreserved),
    ("escape", Kind::ESCAPE, KeywordKind::Unreserved),
    ("event", Kind::EVENT, KeywordKind::Unreserved),
    ("except", Kind::EXCEPT, KeywordKind::Reserved),
    ("exclude", Kind::EXCLUDE, KeywordKind::Unreserved),
    ("excluding", Kind::EXCLUDING, KeywordKind::Unreserved),
    ("exclusive", Kind::EXCLUSIVE, KeywordKind::Unreserved),
    ("execute", Kind::EXECUTE, KeywordKind::Unreserved),
    ("exists", Kind::EXISTS, KeywordKind::ColumnName),
    ("explain", Kind::EXPLAIN, KeywordKind::Unreserved),
    ("expression", Kind::EXPRESSION, KeywordKind::Unreserved),
    ("extension", Kind::EXTENSION, KeywordKind::Unreserved),
    ("external", Kind::EXTERNAL, KeywordKind::Unreserved),
    ("extract", Kind::EXTRACT, KeywordKind::ColumnName),
    ("false", Kind::FALSE_P, KeywordKind::Reserved),
    ("family", Kind::FAMILY, KeywordKind::Unreserved),
    ("fetch", Kind::FETCH, KeywordKind::Reserved),
    ("filter", Kind::FILTER, KeywordKind::Unreserved),
    ("finalize", Kind::FINALIZE, KeywordKind::Unreserved),
    ("first", Kind::FIRST_P, KeywordKind::Unreserved),
    ("float", Kind::FLOAT_P, KeywordKind::ColumnName),
    ("following", Kind::FOLLOWING, KeywordKind::Unreserved),
    ("for", Kind::FOR, KeywordKind::Reserved),
    ("force", Kind::FORCE, KeywordKind::Unreserved),
    ("foreign", Kind::FOREIGN, KeywordKind::Reserved),
    ("format", Kind::FORMAT, KeywordKind::Unreserved),
    ("forward", Kind::FORWARD, KeywordKind::Unreserved),
    ("freeze", Kind::FREEZE, KeywordKind::TypeFunctionName),
    ("from", Kind::FROM, KeywordKind::Reserved),
    ("full", Kind::FULL, KeywordKind::TypeFunctionName),
    ("function", Kind::FUNCTION, KeywordKind::Unreserved),
    ("functions", Kind::FUNCTIONS, KeywordKind::Unreserved),
    ("generated", Kind::GENERATED, KeywordKind::Unreserved),
    ("global", Kind::GLOBAL, KeywordKind::Unreserved),
    ("grant", Kind::GRANT, KeywordKind::Reserved),
    ("granted", Kind::GRANTED, KeywordKind::Unreserved),
    ("greatest", Kind::GREATEST, KeywordKind::ColumnName),
    ("group", Kind::GROUP_P, KeywordKind::Reserved),
    ("grouping", Kind::GROUPING, KeywordKind::ColumnName),
    ("groups", Kind::GROUPS, KeywordKind::Unreserved),
    ("handler", Kind::HANDLER, KeywordKind::Unreserved),
    ("having", Kind::HAVING, KeywordKind::Reserved),
    ("header", Kind::HEADER_P, KeywordKind::Unreserved),
    ("hold", Kind::HOLD, KeywordKind::Unreserved),
    ("hour", Kind::HOUR_P, KeywordKind::Unreserved),
    ("identity", Kind::IDENTITY_P, KeywordKind::Unreserved),
    ("if", Kind::IF_P, KeywordKind::Unreserved),
    ("ignore", Kind::IGNORE_P, KeywordKind::Unreserved),
    ("ilike", Kind::ILIKE, KeywordKind::TypeFunctionName),
    ("immediate", Kind::IMMEDIATE, KeywordKind::Unreserved),
    ("immutable", Kind::IMMUTABLE, KeywordKind::Unreserved),
    ("implicit", Kind::IMPLICIT_P, KeywordKind::Unreserved),
    ("import", Kind::IMPORT_P, KeywordKind::Unreserved),
    ("in", Kind::IN_P, KeywordKind::Reserved),
    ("include", Kind::INCLUDE, KeywordKind::Unreserved),
    ("including", Kind::INCLUDING, KeywordKind::Unreserved),
    ("increment", Kind::INCREMENT, KeywordKind::Unreserved),
    ("indent", Kind::INDENT, KeywordKind::Unreserved),
    ("index", Kind::INDEX, KeywordKind::Unreserved),
    ("indexes", Kind::INDEXES, KeywordKind::Unreserved),
    ("inherit", Kind::INHERIT, KeywordKind::Unreserved),
    ("inherits", Kind::INHERITS, KeywordKind::Unreserved),
    ("initially", Kind::INITIALLY, KeywordKind::Reserved),
    ("inline", Kind::INLINE_P, KeywordKind::Unreserved),
    ("inner", Kind::INNER_P, KeywordKind::TypeFunctionName),
    ("inout", Kind::INOUT, KeywordKind::ColumnName),
    ("input", Kind::INPUT_P, KeywordKind::Unreserved),
    ("insensitive", Kind::INSENSITIVE, KeywordKind::Unreserved),
    ("insert", Kind::INSERT, KeywordKind::Unreserved),
    ("instead", Kind::INSTEAD, KeywordKind::Unreserved),
    ("int", Kind::INT_P, KeywordKind::ColumnName),
    ("integer", Kind::INTEGER, KeywordKind::ColumnName),
    ("intersect", Kind::INTERSECT, KeywordKind::Reserved),
    ("interval", Kind::INTERVAL, KeywordKind::ColumnName),
    ("into", Kind::INTO, KeywordKind::Reserved),
    ("invoker", Kind::INVOKER, KeywordKind::Unreserved),
    ("is", Kind::IS, KeywordKind::TypeFunctionName),
    ("isnull", Kind::ISNULL, KeywordKind::TypeFunctionName),
    ("isolation", Kind::ISOLATION, KeywordKind::Unreserved),
    ("join", Kind::JOIN, KeywordKind::TypeFunctionName),
    ("json", Kind::JSON, KeywordKind::ColumnName),
    ("json_array", Kind::JSON_ARRAY, KeywordKind::ColumnName),
    ("json_arrayagg", Kind::JSON_ARRAYAGG, KeywordKind::ColumnName),
    ("json_exists", Kind::JSON_EXISTS, KeywordKind::ColumnName),
    ("json_object", Kind::JSON_OBJECT, KeywordKind::ColumnName),
    ("json_objectagg", Kind::JSON_OBJECTAGG, KeywordKind::ColumnName),
    ("json_query", Kind::JSON_QUERY, KeywordKind::ColumnName),
    ("json_scalar", Kind::JSON_SCALAR, KeywordKind::ColumnName),
    ("json_serialize", Kind::JSON_SERIALIZE, KeywordKind::ColumnName),
    ("json_table", Kind::JSON_TABLE, KeywordKind::ColumnName),
    ("json_value", Kind::JSON_VALUE, KeywordKind::ColumnName),
    ("keep", Kind::KEEP, KeywordKind::Unreserved),
    ("key", Kind::KEY, KeywordKind::Unreserved),
    ("keys", Kind::KEYS, KeywordKind::Unreserved),
    ("label", Kind::LABEL, KeywordKind::Unreserved),
    ("language", Kind::LANGUAGE, KeywordKind::Unreserved),
    ("large", Kind::LARGE_P, KeywordKind::Unreserved),
    ("last", Kind::LAST_P, KeywordKind::Unreserved),
    ("lateral", Kind::LATERAL_P, KeywordKind::Reserved),
    ("leading", Kind::LEADING, KeywordKind::Reserved),
    ("leakproof", Kind::LEAKPROOF, KeywordKind::Unreserved),
    ("least", Kind::LEAST, KeywordKind::ColumnName),
    ("left", Kind::LEFT, KeywordKind::TypeFunctionName),
    ("level", Kind::LEVEL, KeywordKind::Unreserved),
    ("like", Kind::LIKE, KeywordKind::TypeFunctionName),
    ("limit", Kind::LIMIT, KeywordKind::Reserved),
    ("listen", Kind::LISTEN, KeywordKind::Unreserved),
    ("load", Kind::LOAD, KeywordKind::Unreserved),
    ("local", Kind::LOCAL, KeywordKind::Unreserved),
    ("localtime", Kind::LOCALTIME, KeywordKind::Reserved),
    ("localtimestamp", Kind::LOCALTIMESTAMP, KeywordKind::Reserved),
    ("location", Kind::LOCATION, KeywordKind::Unreserved),
    ("lock", Kind::LOCK_P, KeywordKind::Unreserved),
    ("locked", Kind::LOCKED, KeywordKind::Unreserved),
    ("logged", Kind::LOGGED, KeywordKind::Unreserved),
    ("lsn", Kind::LSN_P, KeywordKind::Unreserved),
    ("mapping", Kind::MAPPING, KeywordKind::Unreserved),
    ("match", Kind::MATCH, KeywordKind::Unreserved),
    ("matched", Kind::MATCHED, KeywordKind::Unreserved),
    ("materialized", Kind::MATERIALIZED, KeywordKind::Unreserved),
    ("maxvalue", Kind::MAXVALUE, KeywordKind::Unreserved),
    ("merge", Kind::MERGE, KeywordKind::Unreserved),
    ("merge_action", Kind::MERGE_ACTION, KeywordKind::ColumnName),
    ("method", Kind::METHOD, KeywordKind::Unreserved),
    ("minute", Kind::MINUTE_P, KeywordKind::Unreserved),
    ("minvalue", Kind::MINVALUE, KeywordKind::Unreserved),
    ("mode", Kind::MODE, KeywordKind::Unreserved),
    ("month", Kind::MONTH_P, KeywordKind::Unreserved),
    ("move", Kind::MOVE, KeywordKind::Unreserved),
    ("name", Kind::NAME_P, KeywordKind::Unreserved),
    ("names", Kind::NAMES, KeywordKind::Unreserved),
    ("national", Kind::NATIONAL, KeywordKind::ColumnName),
    ("natural", Kind::NATURAL, KeywordKind::TypeFunctionName),
    ("nchar", Kind::NCHAR, KeywordKind::ColumnName),
    ("nested", Kind::NESTED, KeywordKind::Unreserved),
    ("new", Kind::NEW, KeywordKind::Unreserved),
    ("next", Kind::NEXT, KeywordKind::Unreserved),
    ("nfc", Kind::NFC, KeywordKind::Unreserved),
    ("nfd", Kind::NFD, KeywordKind::Unreserved),
    ("nfkc", Kind::NFKC, KeywordKind::Unreserved),
    ("nfkd", Kind::NFKD, KeywordKind::Unreserved),
    ("no", Kind::NO, KeywordKind::Unreserved),
    ("none", Kind::NONE, KeywordKind::ColumnName),
    ("normalize", Kind::NORMALIZE, KeywordKind::ColumnName),
    ("normalized", Kind::NORMALIZED, KeywordKind::Unreserved),
    ("not", Kind::NOT, KeywordKind::Reserved),
    ("nothing", Kind::NOTHING, KeywordKind::Unreserved),
    ("notify", Kind::NOTIFY, KeywordKind::Unreserved),
    ("notnull", Kind::NOTNULL, KeywordKind::TypeFunctionName),
    ("nowait", Kind::NOWAIT, KeywordKind::Unreserved),
    ("null", Kind::NULL_P, KeywordKind::Reserved),
    ("nullif", Kind::NULLIF, KeywordKind::ColumnName),
    ("nulls", Kind::NULLS_P, KeywordKind::Unreserved),
    ("numeric", Kind::NUMERIC, KeywordKind::ColumnName),
    ("object", Kind::OBJECT_P, KeywordKind::Unreserved),
    ("objects", Kind::OBJECTS_P, KeywordKind::Unreserved),
    ("of", Kind::OF, KeywordKind::Unreserved),
    ("off", Kind::OFF, KeywordKind::Unreserved),
    ("offset", Kind::OFFSET, KeywordKind::Reserved),
    ("oids", Kind::OIDS, KeywordKind::Unreserved),
    ("old", Kind::OLD, KeywordKind::Unreserved),
    ("omit", Kind::OMIT, KeywordKind::Unreserved),
    ("on", Kind::ON, KeywordKind::Reserved),
    ("only", Kind::ONLY, KeywordKind::Reserved),
    ("operator", Kind::OPERATOR, KeywordKind::Unreserved),
    ("option", Kind::OPTION, KeywordKind::Unreserved),
    ("options", Kind::OPTIONS, KeywordKind::Unreserved),
    ("or", Kind::OR, KeywordKind::Reserved),
    ("order", Kind::ORDER, KeywordKind::Reserved),
    ("ordinality", Kind::ORDINALITY, KeywordKind::Unreserved),
    ("others", Kind::OTHERS, KeywordKind::Unreserved),
    ("out", Kind::OUT_P, KeywordKind::ColumnName),
    ("outer", Kind::OUTER_P, KeywordKind::TypeFunctionName),
    ("over", Kind::OVER, KeywordKind::Unreserved),
    ("overlaps", Kind::OVERLAPS, KeywordKind::TypeFunctionName),
    ("overlay", Kind::OVERLAY, KeywordKind::ColumnName),
    ("overriding", Kind::OVERRIDING, KeywordKind::Unreserved),
    ("owned", Kind::OWNED, KeywordKind::Unreserved),
    ("owner", Kind::OWNER, KeywordKind::Unreserved),
    ("parallel", Kind::PARALLEL, KeywordKind::Unreserved),
    ("parameter", Kind::PARAMETER, KeywordKind::Unreserved),
    ("parser", Kind::PARSER, KeywordKind::Unreserved),
    ("partial", Kind::PARTIAL, KeywordKind::Unreserved),
    ("partition", Kind::PARTITION, KeywordKind::Unreserved),
    ("passing", Kind::PASSING, KeywordKind::Unreserved),
    ("password", Kind::PASSWORD, KeywordKind::Unreserved),
    ("path", Kind::PATH, KeywordKind::Unreserved),
    ("period", Kind::PERIOD, KeywordKind::Unreserved),
    ("placing", Kind::PLACING, KeywordKind::Reserved),
    ("plan", Kind::PLAN, KeywordKind::Unreserved),
    ("plans", Kind::PLANS, KeywordKind::Unreserved),
    ("policy", Kind::POLICY, KeywordKind::Unreserved),
    ("position", Kind::POSITION, KeywordKind::ColumnName),
    ("preceding", Kind::PRECEDING, KeywordKind::Unreserved),
    ("precision", Kind::PRECISION, KeywordKind::ColumnName),
    ("prepare", Kind::PREPARE, KeywordKind::Unreserved),
    ("prepared", Kind::PREPARED, KeywordKind::Unreserved),
    ("preserve", Kind::PRESERVE, KeywordKind::Unreserved),
    ("primary", Kind::PRIMARY, KeywordKind::Reserved),
    ("prior", Kind::PRIOR, KeywordKind::Unreserved),
    ("privileges", Kind::PRIVILEGES, KeywordKind::Unreserved),
    ("procedural", Kind::PROCEDURAL, KeywordKind::Unreserved),
    ("procedure", Kind::PROCEDURE, KeywordKind::Unreserved),
    ("procedures", Kind::PROCEDURES, KeywordKind::Unreserved),
    ("program", Kind::PROGRAM, KeywordKind::Unreserved),
    ("publication", Kind::PUBLICATION, KeywordKind::Unreserved),
    ("quote", Kind::QUOTE, KeywordKind::Unreserved),
    ("quotes", Kind::QUOTES, KeywordKind::Unreserved),
    ("range", Kind::RANGE, KeywordKind::Unreserved),
    ("read", Kind::READ, KeywordKind::Unreserved),
    ("real", Kind::REAL, KeywordKind::ColumnName),
    ("reassign", Kind::REASSIGN, KeywordKind::Unreserved),
    ("recursive", Kind::RECURSIVE, KeywordKind::Unreserved),
    ("ref", Kind::REF_P, KeywordKind::Unreserved),
    ("references", Kind::REFERENCES, KeywordKind::Reserved),
    ("referencing", Kind::REFERENCING, KeywordKind::Unreserved),
    ("refresh", Kind::REFRESH, KeywordKind::Unreserved),
    ("reindex", Kind::REINDEX, KeywordKind::Unreserved),
    ("relative", Kind::RELATIVE_P, KeywordKind::Unreserved),
    ("release", Kind::RELEASE, KeywordKind::Unreserved),
    ("rename", Kind::RENAME, KeywordKind::Unreserved),
    ("repack", Kind::REPACK, KeywordKind::Unreserved),
    ("repeatable", Kind::REPEATABLE, KeywordKind::Unreserved),
    ("replace", Kind::REPLACE, KeywordKind::Unreserved),
    ("replica", Kind::REPLICA, KeywordKind::Unreserved),
    ("reset", Kind::RESET, KeywordKind::Unreserved),
    ("respect", Kind::RESPECT_P, KeywordKind::Unreserved),
    ("restart", Kind::RESTART, KeywordKind::Unreserved),
    ("restrict", Kind::RESTRICT, KeywordKind::Unreserved),
    ("return", Kind::RETURN, KeywordKind::Unreserved),
    ("returning", Kind::RETURNING, KeywordKind::Reserved),
    ("returns", Kind::RETURNS, KeywordKind::Unreserved),
    ("revoke", Kind::REVOKE, KeywordKind::Unreserved),
    ("right", Kind::RIGHT, KeywordKind::TypeFunctionName),
    ("role", Kind::ROLE, KeywordKind::Unreserved),
    ("rollback", Kind::ROLLBACK, KeywordKind::Unreserved),
    ("rollup", Kind::ROLLUP, KeywordKind::Unreserved),
    ("routine", Kind::ROUTINE, KeywordKind::Unreserved),
    ("routines", Kind::ROUTINES, KeywordKind::Unreserved),
    ("row", Kind::ROW, KeywordKind::ColumnName),
    ("rows", Kind::ROWS, KeywordKind::Unreserved),
    ("rule", Kind::RULE, KeywordKind::Unreserved),
    ("savepoint", Kind::SAVEPOINT, KeywordKind::Unreserved),
    ("scalar", Kind::SCALAR, KeywordKind::Unreserved),
    ("schema", Kind::SCHEMA, KeywordKind::Unreserved),
    ("schemas", Kind::SCHEMAS, KeywordKind::Unreserved),
    ("scroll", Kind::SCROLL, KeywordKind::Unreserved),
    ("search", Kind::SEARCH, KeywordKind::Unreserved),
    ("second", Kind::SECOND_P, KeywordKind::Unreserved),
    ("security", Kind::SECURITY, KeywordKind::Unreserved),
    ("select", Kind::SELECT, KeywordKind::Reserved),
    ("sequence", Kind::SEQUENCE, KeywordKind::Unreserved),
    ("sequences", Kind::SEQUENCES, KeywordKind::Unreserved),
    ("serializable", Kind::SERIALIZABLE, KeywordKind::Unreserved),
    ("server", Kind::SERVER, KeywordKind::Unreserved),
    ("session", Kind::SESSION, KeywordKind::Unreserved),
    ("session_user", Kind::SESSION_USER, KeywordKind::Reserved),
    ("set", Kind::SET, KeywordKind::Unreserved),
    ("setof", Kind::SETOF, KeywordKind::ColumnName),
    ("sets", Kind::SETS, KeywordKind::Unreserved),
    ("share", Kind::SHARE, KeywordKind::Unreserved),
    ("show", Kind::SHOW, KeywordKind::Unreserved),
    ("similar", Kind::SIMILAR, KeywordKind::TypeFunctionName),
    ("simple", Kind::SIMPLE, KeywordKind::Unreserved),
    ("skip", Kind::SKIP, KeywordKind::Unreserved),
    ("smallint", Kind::SMALLINT, KeywordKind::ColumnName),
    ("snapshot", Kind::SNAPSHOT, KeywordKind::Unreserved),
    ("some", Kind::SOME, KeywordKind::Reserved),
    ("source", Kind::SOURCE, KeywordKind::Unreserved),
    ("sql", Kind::SQL_P, KeywordKind::Unreserved),
    ("stable", Kind::STABLE, KeywordKind::Unreserved),
    ("standalone", Kind::STANDALONE_P, KeywordKind::Unreserved),
    ("start", Kind::START, KeywordKind::Unreserved),
    ("statement", Kind::STATEMENT, KeywordKind::Unreserved),
    ("statistics", Kind::STATISTICS, KeywordKind::Unreserved),
    ("stdin", Kind::STDIN, KeywordKind::Unreserved),
    ("stdout", Kind::STDOUT, KeywordKind::Unreserved),
    ("storage", Kind::STORAGE, KeywordKind::Unreserved),
    ("stored", Kind::STORED, KeywordKind::Unreserved),
    ("strict", Kind::STRICT_P, KeywordKind::Unreserved),
    ("string", Kind::STRING_P, KeywordKind::Unreserved),
    ("strip", Kind::STRIP_P, KeywordKind::Unreserved),
    ("subscription", Kind::SUBSCRIPTION, KeywordKind::Unreserved),
    ("substring", Kind::SUBSTRING, KeywordKind::ColumnName),
    ("support", Kind::SUPPORT, KeywordKind::Unreserved),
    ("symmetric", Kind::SYMMETRIC, KeywordKind::Reserved),
    ("sysid", Kind::SYSID, KeywordKind::Unreserved),
    ("system", Kind::SYSTEM_P, KeywordKind::Unreserved),
    ("system_user", Kind::SYSTEM_USER, KeywordKind::Reserved),
    ("table", Kind::TABLE, KeywordKind::Reserved),
    ("tables", Kind::TABLES, KeywordKind::Unreserved),
    ("tablesample", Kind::TABLESAMPLE, KeywordKind::TypeFunctionName),
    ("tablespace", Kind::TABLESPACE, KeywordKind::Unreserved),
    ("target", Kind::TARGET, KeywordKind::Unreserved),
    ("temp", Kind::TEMP, KeywordKind::Unreserved),
    ("template", Kind::TEMPLATE, KeywordKind::Unreserved),
    ("temporary", Kind::TEMPORARY, KeywordKind::Unreserved),
    ("text", Kind::TEXT_P, KeywordKind::Unreserved),
    ("then", Kind::THEN, KeywordKind::Reserved),
    ("ties", Kind::TIES, KeywordKind::Unreserved),
    ("time", Kind::TIME, KeywordKind::ColumnName),
    ("timestamp", Kind::TIMESTAMP, KeywordKind::ColumnName),
    ("to", Kind::TO, KeywordKind::Reserved),
    ("trailing", Kind::TRAILING, KeywordKind::Reserved),
    ("transaction", Kind::TRANSACTION, KeywordKind::Unreserved),
    ("transform", Kind::TRANSFORM, KeywordKind::Unreserved),
    ("treat", Kind::TREAT, KeywordKind::ColumnName),
    ("trigger", Kind::TRIGGER, KeywordKind::Unreserved),
    ("trim", Kind::TRIM, KeywordKind::ColumnName),
    ("true", Kind::TRUE_P, KeywordKind::Reserved),
    ("truncate", Kind::TRUNCATE, KeywordKind::Unreserved),
    ("trusted", Kind::TRUSTED, KeywordKind::Unreserved),
    ("type", Kind::TYPE_P, KeywordKind::Unreserved),
    ("types", Kind::TYPES_P, KeywordKind::Unreserved),
    ("uescape", Kind::UESCAPE, KeywordKind::Unreserved),
    ("unbounded", Kind::UNBOUNDED, KeywordKind::Unreserved),
    ("uncommitted", Kind::UNCOMMITTED, KeywordKind::Unreserved),
    ("unconditional", Kind::UNCONDITIONAL, KeywordKind::Unreserved),
    ("unencrypted", Kind::UNENCRYPTED, KeywordKind::Unreserved),
    ("union", Kind::UNION, KeywordKind::Reserved),
    ("unique", Kind::UNIQUE, KeywordKind::Reserved),
    ("unknown", Kind::UNKNOWN, KeywordKind::Unreserved),
    ("unlisten", Kind::UNLISTEN, KeywordKind::Unreserved),
    ("unlogged", Kind::UNLOGGED, KeywordKind::Unreserved),
    ("until", Kind::UNTIL, KeywordKind::Unreserved),
    ("update", Kind::UPDATE, KeywordKind::Unreserved),
    ("user", Kind::USER, KeywordKind::Reserved),
    ("using", Kind::USING, KeywordKind::Reserved),
    ("vacuum", Kind::VACUUM, KeywordKind::Unreserved),
    ("valid", Kind::VALID, KeywordKind::Unreserved),
    ("validate", Kind::VALIDATE, KeywordKind::Unreserved),
    ("validator", Kind::VALIDATOR, KeywordKind::Unreserved),
    ("value", Kind::VALUE_P, KeywordKind::Unreserved),
    ("values", Kind::VALUES, KeywordKind::ColumnName),
    ("varchar", Kind::VARCHAR, KeywordKind::ColumnName),
    ("variadic", Kind::VARIADIC, KeywordKind::Reserved),
    ("varying", Kind::VARYING, KeywordKind::Unreserved),
    ("verbose", Kind::VERBOSE, KeywordKind::TypeFunctionName),
    ("version", Kind::VERSION_P, KeywordKind::Unreserved),
    ("view", Kind::VIEW, KeywordKind::Unreserved),
    ("views", Kind::VIEWS, KeywordKind::Unreserved),
    ("virtual", Kind::VIRTUAL, KeywordKind::Unreserved),
    ("volatile", Kind::VOLATILE, KeywordKind::Unreserved),
    ("wait", Kind::WAIT, KeywordKind::Unreserved),
    ("when", Kind::WHEN, KeywordKind::Reserved),
    ("where", Kind::WHERE, KeywordKind::Reserved),
    ("whitespace", Kind::WHITESPACE_P, KeywordKind::Unreserved),
    ("window", Kind::WINDOW, KeywordKind::Reserved),
    ("with", Kind::WITH, KeywordKind::Reserved),
    ("within", Kind::WITHIN, KeywordKind::Unreserved),
    ("without", Kind::WITHOUT, KeywordKind::Unreserved),
    ("work", Kind::WORK, KeywordKind::Unreserved),
    ("wrapper", Kind::WRAPPER, KeywordKind::Unreserved),
    ("write", Kind::WRITE, KeywordKind::Unreserved),
    ("xml", Kind::XML_P, KeywordKind::Unreserved),
    ("xmlattributes", Kind::XMLATTRIBUTES, KeywordKind::ColumnName),
    ("xmlconcat", Kind::XMLCONCAT, KeywordKind::ColumnName),
    ("xmlelement", Kind::XMLELEMENT, KeywordKind::ColumnName),
    ("xmlexists", Kind::XMLEXISTS, KeywordKind::ColumnName),
    ("xmlforest", Kind::XMLFOREST, KeywordKind::ColumnName),
    ("xmlnamespaces", Kind::XMLNAMESPACES, KeywordKind::ColumnName),
    ("xmlparse", Kind::XMLPARSE, KeywordKind::ColumnName),
    ("xmlpi", Kind::XMLPI, KeywordKind::ColumnName),
    ("xmlroot", Kind::XMLROOT, KeywordKind::ColumnName),
    ("xmlserialize", Kind::XMLSERIALIZE, KeywordKind::ColumnName),
    ("xmltable", Kind::XMLTABLE, KeywordKind::ColumnName),
    ("year", Kind::YEAR_P, KeywordKind::Unreserved),
    ("yes", Kind::YES_P, KeywordKind::Unreserved),
    ("zone", Kind::ZONE, KeywordKind::Unreserved),
];
