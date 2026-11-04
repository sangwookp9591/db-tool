# AxDB ETL 경쟁 도구 조사 (기준일 2026-11-04)

출처 표기: [공식] 공식 문서/릴리스 노트/GitHub 릴리스, [독립] 독립 벤치마크, [2차] 블로그/보도자료/요약.
주의: 일부 항목은 WebFetch 요약 결과에 근거하며, 확인 불가 항목은 "미확인"으로 명시했다. 사실 확인이 약한 곳은 "(검증 필요)" 표기.

## 1. 도구별 요약

### 1.1 Apache Airflow
- 최신: **3.3.2** (패치), 2026-09-17. 최신 마이너 3.3.0은 2026-07-06. [공식] https://github.com/apache/airflow/releases , https://airflow.apache.org/docs/apache-airflow/stable/release_notes.html
- 실행 모델: Python으로 작성하는 DAG + 스케줄러/워커(오케스트레이터). 데이터를 직접 옮기지 않고 태스크(오퍼레이터)를 실행. 런타임 Python (3.3에서 Java/Go Task SDK 추가). [공식]
- ETL 관련 기능:
  - 커넥터: 수백 개 provider 패키지(DB, 클라우드 등). 변환 스텝 카탈로그는 없고 코드/SQL 오퍼레이터에 위임. [공식 일반 지식, 이번 조사에서 URL 재확인 안 함]
  - 스케줄/트리거: cron, 데이터셋/Asset 기반 트리거, 백필. Asset partitioning(3.2/3.3). [공식]
  - 의존성: DAG 구조 자체. 에러 처리: 태스크 재시도, 3.3에서 "pluggable retry policies". Deadline Alerts(콜백 지원). [공식]
  - 증분/CDC: 내장 없음(watermark는 Variable/Asset state로 구현). 3.3의 "task and asset state store"가 이를 보조. [공식, 해석은 2차]
  - 행 단위 reject/dead-letter: 없음(태스크 단위 성공/실패).
  - 관측성: UI, OpenTelemetry 메트릭(3.3에서 histogram 전환). 버전관리: Dag bundle 버전 추적(clear/rerun/backfill 시). 시크릿: Secrets Backend. 테스트: 코드 단위 테스트(`dag.test()`).
- 최신 릴리스 주요 신기능(3.3.0): Asset partitioning, task/asset state store, 플러거블 재시도 정책, Java/Go Task SDK, Dag bundle 버전 추적, Deadlines 페이지. 3.2.0(2026-04-07): 멀티팀, SQLAlchemy 2.0, Python 3.14. [공식]
- 성능 수치: 공식/독립 rows/s 수치 없음(스케줄러 도구라 비교 대상 아님). 신뢰할 만한 수치 없음.

### 1.2 Apache NiFi
- 최신: **2.12.0**, 2026-09-13. [공식] https://nifi.apache.org/download/ , https://cwiki.apache.org/confluence/display/NIFI/Release+Notes . 1.x 최종은 1.28.1(지원 종료 2024-12-08 표기).
- 실행 모델: 플로우 기반 프로그래밍(FlowFile, 프로세서, 커넥션 큐, 백프레셔), 웹 UI 시각 편집, 노드 클러스터. 런타임 Java. [공식]
- 기능: 수백 개 프로세서(DB, Kafka, S3/GCS/Azure, Iceberg 등), Record 기반 프로세서(QueryRecord 등 SQL), 스케줄링(타이머/cron), 연결 큐 백프레셔 + back-pressure 임계값, FlowFile 단위 relationship(failure/retry)으로 dead-letter 라우팅 가능, 데이터 출처(Data Provenance)로 리니지/재생, Parameter Context + Parameter Provider(AWS Secrets Manager 등)로 시크릿, Registry/git 기반 버전관리(2.8에서 NiFi Registry deprecated, git 레지스트리 클라이언트 전환). CDC는 CaptureChangeMySQL 등 일부 프로세서. [공식 릴리스 노트로 확인된 것: 2.7~2.12 항목, 나머지는 일반 지식]
- 최신 신기능: 2.12 SQS 가시성 타임아웃 프로세서, PropertyEncryptionProvider; 2.11 Registry rebase, 백로그 보고; 2.10 Registry 브랜치 생성, Connectors UI; 2.9 Connectors 초기 지원. [공식]
- 성능: 공식 보장 수치 없음. 한 벤더성 글이 "단일 노드 약 50MB/s(일반 디스크)"를 언급하나 방법론 없음 [2차, 신뢰 낮음].

### 1.3 Apache Hop
- 최신: **2.20.0**, 2026-11-02 (다운로드 페이지 표기, Java 21 필요). GitHub 릴리스 태그는 2.20.0-rc1(2026-10-27). [공식] https://hop.apache.org/download/ , https://github.com/apache/hop/releases
- 실행 모델: 시각 편집(Hop GUI) 파이프라인(변환 스텝 그래프, 행 스트림) + 워크플로(제어 흐름). 메타데이터 중심, 프로젝트/환경 개념, 로컬/Spark/Flink/Beam 엔진 선택. 런타임 Java. Kettle에서 포크. [공식]
- 기능: 100+ 변환(Table Input/Output, Select values, Filter rows, Value mapper, Sort/Unique, Merge join, Calculator, Add constants 등), 다수 DB 커넥터와 file/object 스토리지, 파이프라인 단위 "error handling" 출력(행 단위 reject 스트림), 파이프라인 단위 단위 테스트와 데이터 셋(golden data), 프리뷰/디버그, Git 연동, 환경 변수/시크릿(Vault, AWS Secrets Manager 등), Hop Server/Web. [공식 일반 지식 + 2.20 노트]
- 2.20 신기능: Hop Web 인증/롤, 신규 변환(Spark SQL, JMS, Multi Mapping, Git Input, Text Chunker, Embed Text, pgvector upsert/search 등), Database perspective, Iceberg 읽기/쓰기, OpenLineage 이벤트, 파이프라인/워크플로 linter, Vault/OpenBao TLS 기본 검증. [공식] https://github.com/apache/hop/releases/tag/2.20.0-rc1
- 성능: 공식 벤치마크 없음. 커뮤니티 사례: SQL Server 500만 행 적재, commit size 50,000에서 3분 11초 vs SSIS 2분 11초 (약 26k vs 38k rows/s, 환경 불명, 사용자 보고) [2차] https://github.com/apache/hop/discussions/7984 . 메인테이너는 병목 확인을 먼저 권고.

### 1.4 Pentaho Data Integration (PDI / Kettle)
- 최신: **Pentaho Data Integration & Analytics 11.0 (LTS)**, 발표 2026-02-04. 11.0.x 서비스팩 존재 여부와 정확한 GA 일은 이번 조사에서 확인하지 못함(문서에 날짜 없음). [공식] https://docs.pentaho.com/whats-new , https://docs.pentaho.com/whats-new/release-notes-11.0 ; 날짜는 [2차] https://www.placera.se/pressmeddelanden/pentaho-enhances-flagship-data-integration-solution-to-deliver-operational-and-ai-ready-data-at-scale-20260204
- 실행 모델: Spoon 시각 편집(변환 = 행 스트림 병렬 스레드, 잡 = 순차 제어), Pentaho Server/Carte 원격 실행. 런타임 Java(11부터 Java 21 지원). [공식]
- 기능: 광범위한 스텝 카탈로그(Kettle 유산), 스케줄링(서버), 스텝 에러 처리(행 단위 error hop), 로깅/모니터링, 엔터프라이즈 에디션은 Git/버전관리와 보안 기능. CDC는 별도 스텝/DB 로그 기반(일반 지식).
- 11.0 신기능: 브라우저 기반 Pipeline Designer(Spoon 없이 작성), Project Lifecycle Management, Plugin Manager, Karaf/OSGi 제거(빅데이터 컴포넌트 플러그인화, 1GB 이상 축소), OTEL 기반 관측성, Java 21, OAuth2/OIDC(서버), Docker/K8s 이미지. [공식]
- 성능: 공식 수치 미확인. 업계 통설(Table Output의 batch/commit size가 핵심)만 존재.

### 1.5 Qlik Talend Studio
- 최신: 월간 릴리스 **R2026-10** (Studio 8.0.1 누적 패치 Patch_20261023_R2026-10, 2026-10-23); 신기능 문서가 정리된 최신 기능 릴리스는 **R2026-09**. [공식/준공식] https://update.talend.com/Studio/8/updates/latest/PATCH_RELEASE_NOTE.html , https://help.qlik.com/talend/en-US/release-notes/8.0/r2026-09-studio-new-features . R2026-09 정확한 일자는 확인 못함.
- 참고: Talend Open Studio(무료판)는 2026-01-31 종료 [2차] https://www.dataexpert.io/blog/open-source-etl-tools-comparison-guide-2026 (검증 필요). 상용 Studio만 지속.
- 실행 모델: 시각 디자이너에서 잡을 설계 → Java 코드 생성/컴파일 → 독립 실행. 런타임 Java(2026-06부터 Studio 실행에 Java 21 필요, 잡 런타임은 17/21). [공식/2차] 
- 기능: 1000+ 컴포넌트(tMap, tFilterRow, tAggregate 등), DB/클라우드/SaaS 커넥터, 스케줄링은 Talend Management Console/TAC, 행 단위 reject 출력(tMap reject, 컴포넌트별 REJECT 링크), 데이터 품질/마스킹 컴포넌트, Git 연동 및 CI Builder, 컨텍스트 변수/시크릿, CDC는 별도 구독 기능, 디버그/Traces Debug.
- R2026-09 신기능: AI 어시스턴트(Qlik Answers, 프리뷰: 잡 설명/Java·SQL 생성/tMap 매핑/런타임 오류 진단), SOAP 마이크로서비스 빌드, Spark Structured Streaming(프리뷰, Java 21), SharePoint 컴포넌트, Data Stewardship delta 추출, tGreenplum 개선, CI Builder 8.0.39. R2026-10: SAP OData HTTP 헤더, 업그레이드 안내/역방향 업그레이드 차단. [공식]
- 성능: 공식 벤치마크 없음. 커뮤니티 사례: Oracle→MySQL 약 617 rows/s, MySQL→MySQL 약 4,345 rows/s (Jasper ETL, 이기종 환경, commit 10k vs 50k에서 50k가 오히려 느림) [2차] https://community.qlik.com/t5/Talend-Studio/Issue-Clarification-on-TOS-vs-TIS-TDQ/td-p/2354916 . 환경 불명이라 비교 근거로 부적합.

### 1.6 Kestra
- 최신: **v2.0.9**, 2026-11-03 (1.3 라인 v1.3.46도 같은 날). 2.0.0은 9월 7일(연도는 페이지에 미표기, 연속 태그 날짜로 2026 추정). [공식] https://github.com/kestra-io/kestra/releases
- 실행 모델: YAML 선언형 플로우(inputs, tasks, triggers), 이벤트 기반 + 스케줄, 플러그인 태스크(Java 코어, Kafka/JDBC 백엔드, 워커). 데이터는 대부분 외부 시스템/스토리지 경유(내부 스토리지). [공식]
- 기능: 1000+ 플러그인(DB, 클라우드, dbt 등), 스케줄/웹훅/실시간/Debezium 트리거(CDC; 1.0부터 offset 저장 명시 필요), 재시도·타임아웃·에러 핸들러, 서브플로우, 동시성 제한, Flow SLA, 리플레이, 네임스페이스 파일/KV, Git 동기화(Enterprise 일부), 시크릿 관리, 블루프린트, AI Copilot, Assets(데이터 자산 의존 그래프). 행 단위 reject/dead-letter는 없음(태스크 단위). [공식 릴리스 노트로 확인한 것: 1.0, 2.0 항목]
- 2.0.0: RFC 9457 오류 포맷(파괴적 변경), No-Code 에디터 AI Copilot, Assets DAG 뷰/신선도, UI 개편, CSRF/ZIP-bomb 방어, secret 속성 타입. 1.0.0: Singer 제거, 예약어 flow ID 제한, Helm 구조 개편, 워커 스레드 8x CPU. [공식] https://github.com/kestra-io/kestra/releases/tag/v2.0.0 , .../v1.0.0
- 성능: 공식 수치(실행/초 등)는 이번 조사에서 확인 못함. rows/s 수치 없음.

## 2. A) 필수 ETL 기능 표 (AxDB v1 대비)

| 기능 | Airflow | NiFi | Hop | PDI | Talend | Kestra | AxDB v1 |
|---|---|---|---|---|---|---|---|
| 소스 -> 변환 -> 타깃 파이프라인 정의 | 코드 | O | O | O | O | YAML | Yes |
| 시각적 변환 스텝 카탈로그 | X | 프로세서 | O | O | O | X | Partial (9종 선형) |
| 변환: select/rename/cast/filter/constant/trim/fill_null/concat/dedupe | X(코드) | Record 프로세서 | O | O | O(tMap 등) | X | Yes (선형만) |
| 조인/집계/정렬/lookup | X(SQL) | O | O | O | O | X(SQL) | No (SQL 변환 Planned) |
| 다중 소스/분기/병합(DAG) | O | O | O | O | O | O | No (DAG Planned) |
| 배치 적재 + 트랜잭션 | 태스크 책임 | 프로세서별 | commit size | commit size | commit 간격 | 태스크 책임 | Yes (원자적 배치) |
| 타깃 테이블 자동 생성 | X | 일부 | 일부 | 일부 | 일부 | X | Yes |
| 행 단위 오류 처리(reject/skip) | X | 관계 라우팅 | error hop | error hop/reject | reject 링크 | X | Partial (skip/abort, 별도 reject 보관 없음) |
| 재시도/백오프 | O | O | 제한적 | 제한적 | 제한적 | O | No |
| 스케줄/트리거 | O | O | 서버 | 서버 | TMC/TAC | O | Planned |
| 증분(watermark)/CDC | 직접 구현 | 일부 CDC | 직접/스텝 | 직접/스텝 | 있음(구독) | Debezium 트리거 | Planned |
| 데이터 프리뷰/디버그 | X | 큐 검사 | O | O | O | 출력 보기 | Partial (롤백 미리보기만, 스텝별 프리뷰 확인 필요) |
| 취소/진행률/실행 이력 | O | O | O | O | O | O | Yes |
| 리니지/관측성 | OTel | Provenance | OpenLineage | OTEL | 일부 | Assets/메트릭 | No (실행 이력만) |
| 버전관리/GitOps | Dag bundle | git 레지스트리 | Git | 유료 Git | Git | Git 동기화 | No (명세 파일 export로 시작 가능) |
| 시크릿 관리 | O | Parameter Provider | Vault 등 | O | 컨텍스트 | O | 기존 연결 자격증명 재사용 (확인 필요) |
| 병렬/백프레셔 | 워커 | O | O | 스텝 병렬 | O | 워커 | No (단일 순차 의도) |
| 파이프라인 테스트 | 코드 | 제한적 | 단위 테스트 | 제한적 | 제한적 | 제한적 | No |
| 롤백 미리보기 / dry-run | X | X | X | X | X | X | Yes (차별점) |
| 파일 소스/타깃 | O | O | O | O | O | O | Planned |

(표의 경쟁사 칸은 위 출처와 일반 지식에 근거하며, 칸 단위로 전부 공식 문서에서 재확인하지는 않았다.)

## 3. B) v1의 상위 5개 격차 (데스크톱 DB 도구 사용자 기준)

1. **스텝별 데이터 프리뷰/샘플 실행**: Hop/PDI/Talend는 변환 중간 결과를 즉시 본다. 데스크톱 사용자는 "몇 행으로 돌려보고 고치는" 루프가 핵심이며, 롤백 미리보기만으로는 변환 설계 단계 피드백이 부족하다.
2. **조인/집계/SQL 변환 스텝**: 실사용 ETL의 상당수가 조인·집계다. 선형 9종만으로는 "한 테이블 정리 복사"에 한정된다. 최소한 SQL 변환 스텝이나 소스 SQL 조인으로 우회 가능함을 명확히 해야 한다.
3. **reject 행 보관 및 재처리**: skip/abort만 있고 거부 행과 사유를 파일/테이블로 저장하지 못하면 데이터 손실을 알아채기 어렵다. 경쟁 도구의 표준 기능(error hop, reject 링크)이다.
4. **증분(watermark) 적재와 upsert/머지 모드**: 반복 실행이 일상인 DB 도구에서 전체 재적재(append/truncate만)는 비효율·위험. 타깃 쓰기 모드(append/truncate/upsert)와 watermark를 로드맵 앞쪽에 두는 편이 낫다.
5. **파이프라인 명세의 이식성 및 버전관리(파일 export/Git 친화) + 스케줄**: 단일 사용자라도 정의를 파일로 저장해 diff/백업/공유하고, 앱이 켜져 있는 동안의 스케줄 실행은 기대치가 높다. Kestra(YAML)/Hop(파일 기반 메타데이터)가 보여주는 방식이다.

(차순위: 재시도, 이종 DB 간 타입 매핑 경고, 대량 적재 시 COPY/LOAD 계열 고속 경로.)

## 4. C) 성능 비교 가이드

- 확인된 사실: 6개 도구 모두 공식 rows/s 벤치마크를 공개하지 않았고, 도구 간 동일 조건 독립 벤치마크도 이번 검색에서 찾지 못했다. 신뢰할 만한 기준 수치는 없다고 보는 것이 맞다. [독립 검색 결과 없음]
- 참고용 단편 수치(모두 환경 불명, 2차):
  - Hop 사용자 보고: 500만 행, commit 50k, 3m11s(약 26k rows/s), SSIS 2m11s. https://github.com/apache/hop/discussions/7984
  - Talend(Jasper ETL) 커뮤니티: MySQL→MySQL 약 4,345 rows/s, Oracle→MySQL 약 617 rows/s. https://community.qlik.com/t5/Talend-Studio/Issue-Clarification-on-TOS-vs-TIS-TDQ/td-p/2354916
  - NiFi 단일 노드 약 50MB/s 주장(방법론 없음, 신뢰 낮음).
  - Meltano/PostgreSQL ELT 테스트: 테이블에 따라 7~2,399 rows/s로 편차가 커서 MB/s가 더 안정적인 지표라는 결론 [2차] https://medium.com/@pp_85623/testing-performance-of-elt-data-ingest-meltano-oracle-and-postgresql-471989ff82df
- 공정한 비교 지표 제안:
  1. 처리량은 rows/s와 MB/s를 함께 보고(행 폭이 다르면 rows/s는 오도됨).
  2. 시나리오 고정: 동일 DB 인스턴스/로컬 네트워크, 동일 스키마(좁은 행 5컬럼, 넓은 행 50컬럼+TEXT), 1M~10M 행, 인덱스 유무, 타깃 테이블 비어있음/있음.
  3. 변수 통제: 배치 크기(1k/5k/10k), 커밋 간격, 병렬도 1 고정, 워밍업 후 3회 중앙값, 소스 읽기 페이징 방식(keyset vs offset).
  4. 기준선 대조: 같은 DB 쌍에서 `COPY`/`INSERT ... SELECT`(같은 서버)/pg_dump 파이프를 상한선으로 두고, 도구 효율 = 도구 처리량 / 상한선으로 표시.
  5. 부가 지표: 피크 메모리(RSS), CPU, 콜드 스타트 시간(Hop/PDI/Talend는 JVM 기동 포함), 취소 응답 지연, 롤백 소요 시간.
  6. 비교 대상은 "Hop/PDI의 Table Input→Table Output 한 쌍" 같은 최소 파이프라인으로 제한. Airflow/Kestra/NiFi는 오케스트레이터/플로우 엔진이라 행 복사 벤치마크 대상으로 부적절하며, 대신 태스크 오버헤드(기동 지연)로 비교한다.
  7. 결과 공개 시 하드웨어, 버전(위 최신 버전), 드라이버, 설정 전체를 함께 기록. 자체 측정이므로 "[자체 측정]"으로 표기.
