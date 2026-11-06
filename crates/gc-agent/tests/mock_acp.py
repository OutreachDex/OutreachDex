import json,sys,time
mode=sys.argv[1]
def send(value):
 print(json.dumps(dict(jsonrpc='2.0',**value)),flush=True)
for line in sys.stdin:
 msg=json.loads(line); method=msg.get('method'); ident=msg.get('id')
 if method=='initialize': send(dict(id=ident,result={'protocolVersion':1,'agentCapabilities':{},'authMethods':[]}))
 elif method=='session/new':
  if mode=='generation':
   assert msg['params']['mcpServers']==[]
   assert msg['params']['_meta']['claudeCode']['options']['tools']==[]
   assert msg['params']['_meta']['claudeCode']['options']['settingSources']==[]
  send(dict(id=ident,result={'sessionId':'test-session'}))
 elif method=='session/prompt':
  if mode=='crash': sys.exit(7)
  send(dict(method='session/update',params={'sessionId':'test-session','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'Streaming '}}}))
  if mode=='cancel': time.sleep(60); continue
  send(dict(id=900,method='session/request_permission',params={'sessionId':'test-session','toolCall':{'toolCallId':'tool-1','title':'Mock write','kind':'edit','status':'pending'},'options':[{'optionId':'allow','name':'Allow once','kind':'allow_once'},{'optionId':'deny','name':'Reject once','kind':'reject_once'}]}))
  answer=json.loads(next(sys.stdin))
  if mode=='generation': assert answer['result']['outcome']['outcome']=='cancelled'
  else: assert answer['result']['outcome']['optionId']==mode
  send(dict(method='session/update',params={'sessionId':'test-session','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':'complete.'}}}))
  send(dict(id=ident,result={'stopReason':'end_turn'}))
