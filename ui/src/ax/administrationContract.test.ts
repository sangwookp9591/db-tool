import {describe,it,expect} from 'vitest';
import {administrationMock} from './administrationContract';
import {validate} from '../agent/webmcp';
describe('administration boundary',()=>{
 it('marks fixture privileges as mock and refuses fake backup files',async()=>{const roles=await administrationMock('roles',{}) as {roles:{summary:string[]}[]};expect(roles.roles[0].summary[0]).toContain('모의');await expect(administrationMock('backup_create',{})).rejects.toMatchObject({kind:'Unsupported'});});
 it('refuses fake streaming files and advertises no mock database creation',async()=>{for(const command of ['backup_create_file','restore_pick_file','restore_file_preview','restore_file_apply'])await expect(administrationMock(command,{})).rejects.toMatchObject({kind:'Unsupported'});expect(await administrationMock('restore_options',{})).toMatchObject({overwrite:false,new_database:false});});
 it('validates nested grant requests and rejects unexpected approval inputs',()=>{const schema={type:'array',items:{type:'object',properties:{role:{type:'string'}},required:['role'],additionalProperties:false}};expect(()=>validate(schema,[{role:'analyst'}])).not.toThrow();expect(()=>validate(schema,[{role:'analyst',approved:true}])).toThrow();expect(()=>validate(schema,{})).toThrow();expect(()=>validate(schema,Array(101).fill({role:'analyst'}))).toThrow();});
});
